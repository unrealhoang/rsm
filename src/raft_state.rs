use crate::utils;
use crate::LogEntry;
use crate::{raft_network::TimerKind, Log, RaftNetwork, StateMachine};
use std::fmt::Debug;
use std::ops::Index;

#[derive(Debug)]
pub struct AppendEntries<E> {
    term: u64,
    leader_id: u64,
    prev_log_index: u64,
    prev_log_term: u64,
    entries: Vec<LogEntry<E>>,
    leader_commit: u64,
}

#[derive(Debug)]
pub struct AppendEntriesResponse {
    last_index: u64,
    term: u64,
    success: bool,
}

#[derive(Debug)]
pub struct RequestVote {
    term: u64,
    candidate_id: u64,
    last_log_index: u64,
    last_log_term: u64,
}

#[derive(Debug)]
pub struct RequestVoteResponse {
    term: u64,
    vote_granted: bool,
}

#[derive(Debug)]
pub enum Msg<E> {
    AppendEntries(AppendEntries<E>),
    AppendEntriesResponse(AppendEntriesResponse),
    RequestVote(RequestVote),
    RequestVoteResponse(RequestVoteResponse),
}

impl<E> Msg<E> {
    fn term(&self) -> u64 {
        match self {
            Msg::AppendEntries(a) => a.term,
            Msg::AppendEntriesResponse(a) => a.term,
            Msg::RequestVote(a) => a.term,
            Msg::RequestVoteResponse(a) => a.term,
        }
    }
}

#[derive(Debug)]
pub(crate) enum RoleState {
    Follower,
    Leader {
        next_indexes: PeerIndexes,
        match_indexes: PeerIndexes,
    },
    Candidate {
        // Stores peers's vote
        votes: PeerInfos<bool>,
    },
}

pub type PeerIndexes = PeerInfos<u64>;

#[derive(Debug)]
pub struct PeerInfos<T: Copy + Debug>(Vec<(u64, T)>);

impl<T: Copy + Debug> PeerInfos<T> {
    pub(crate) fn new() -> Self {
        PeerInfos(Vec::new())
    }

    pub(crate) fn insert(&mut self, peer_id: u64, data: T) {
        if let Some(ref mut item) = self.0.iter_mut().find(|item| item.0 == peer_id) {
            item.1 = data;
        } else {
            self.0.push((peer_id, data));
        }
    }

    pub(crate) fn keys(&self) -> impl Iterator<Item = u64> + '_ + Clone + ExactSizeIterator {
        self.0.iter().map(|item| item.0)
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = T> + '_ + Clone + ExactSizeIterator {
        self.0.iter().map(|item| item.1)
    }
}

impl Index<u64> for PeerIndexes {
    type Output = u64;
    fn index(&self, peer_id: u64) -> &Self::Output {
        &self.0.iter().find(|item| item.0 == peer_id).unwrap().1
    }
}

pub(crate) enum Suffrage {
    Voter,
    Nonvoter,
    Staging,
}

pub(crate) struct PeerConfig {
    pub(crate) id: u64,
    pub(crate) suffrage: Suffrage,
}

pub(crate) struct Configuration {
    peers: Vec<PeerConfig>,
}

pub(crate) struct RaftState<SM, L>
where
    SM: StateMachine,
    L: Log<Event = SM::Event>,
{
    id: u64,
    configuration: Configuration,

    // Persistent state
    curr_term: u64,
    voted_for: Option<u64>,
    log: L,
    sm: SM,

    // Volatile state
    commit_index: u64,
    last_applied: u64,
    role_state: RoleState,
}

impl<SM, L> Debug for RaftState<SM, L>
where
    SM: StateMachine,
    L: Log<Event = SM::Event>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = match self.role_state {
            RoleState::Leader { .. } => "leader",
            RoleState::Follower { .. } => "follower",
            RoleState::Candidate { .. } => "candidate",
        };

        f.debug_struct("RaftState")
            .field("curr_term", &self.curr_term)
            .field("voted_for", &self.voted_for)
            .field("commit_index", &self.commit_index)
            .field("last_applied", &self.last_applied)
            .field("role_state", &state)
            .finish()
    }
}

fn update_peer_request<L, E>(
    log: &L,
    id: u64,
    curr_term: u64,
    commit_index: u64,
    next_indexes: &PeerIndexes,
    peer_id: u64,
) -> AppendEntries<E>
where
    L: Log<Event = E>,
    E: Clone,
{
    let entries = log.slice_to_end(next_indexes[peer_id]).to_vec();
    let prev_entry = log.at(next_indexes[peer_id] - 1);
    let request = AppendEntries {
        term: curr_term,
        leader_id: id,
        prev_log_index: prev_entry.map(|e| e.index).unwrap_or(0),
        prev_log_term: prev_entry.map(|e| e.term).unwrap_or(0),
        entries,
        leader_commit: commit_index,
    };
    return request;
}

impl<SM, L> RaftState<SM, L>
where
    SM: StateMachine,
    L: Log<Event = SM::Event>,
{
    pub(crate) fn new(id: u64, sm: SM, log: L, peers: Vec<PeerConfig>) -> Self {
        RaftState {
            id,

            curr_term: 0,
            voted_for: None,
            log,

            commit_index: 0,
            last_applied: 0,
            sm,
            role_state: RoleState::Follower,
            configuration: Configuration { peers },
        }
    }

    pub(crate) fn apply_committed(&mut self) -> bool {
        if self.commit_index > self.last_applied {
            let events = self
                .log
                .slice(self.last_applied + 1, self.commit_index + 1)
                .iter()
                .map(|entry| &entry.data);
            // TODO: async SM execution by sending committed index over channel
            self.sm.execute_batch(events);
            self.last_applied = self.commit_index;
            true
        } else {
            false
        }
    }

    pub(crate) fn update_peers<N>(&self, net: &mut N, is_heartbeat: bool) -> Result<(), ()>
    where
        N: RaftNetwork<Event = SM::Event>,
    {
        if let RoleState::Leader { next_indexes, .. } = &self.role_state {
            let append_reqs = self
                .configuration
                .peers
                .iter()
                .filter(|p| p.id != self.id)
                .map(|peer| {
                    (
                        peer.id,
                        update_peer_request(
                            &self.log,
                            self.id,
                            self.curr_term,
                            self.commit_index,
                            &next_indexes,
                            peer.id,
                        ),
                    )
                })
                .filter_map(|(id, append)| {
                    if append.entries.len() > 0 || is_heartbeat {
                        Some((id, Msg::AppendEntries(append)))
                    } else {
                        None
                    }
                });

            net.send_all(append_reqs)
        } else {
            Err(())
        }
    }

    pub(crate) fn is_leader(&self) -> bool {
        matches!(self.role_state, RoleState::Leader { .. })
    }

    pub(crate) fn propose(&mut self, request: SM::Event) {
        // drop client request if not Leader
        if !self.is_leader() {
            log::info!("[{:?}] Drop client request", self.role_state);
        } else {
            self.log.push(self.curr_term, request);
        }
    }

    pub(crate) fn election_timeout<N>(&mut self, net: &mut N)
    where
        N: RaftNetwork<Event = SM::Event>,
    {
        log::info!(
            "[{}] Election timeout, self promote. D: {:#?}",
            self.id,
            self
        );
        self.curr_term += 1;
        let mut votes = PeerInfos::new();
        self.voted_for = Some(self.id);
        votes.insert(self.id, true);
        self.role_state = RoleState::Candidate { votes };

        let self_promote_msgs = self
            .configuration
            .peers
            .iter()
            .filter(|p| p.id != self.id)
            .map(|peer| {
                (
                    peer.id,
                    Msg::RequestVote(RequestVote {
                        term: self.curr_term,
                        candidate_id: self.id,
                        last_log_index: self.log.last_index(),
                        last_log_term: self.log.last_term(),
                    }),
                )
            });

        net.send_all(self_promote_msgs);
        net.timer_reset(TimerKind::Election);
    }

    pub(crate) fn heartbeat_timeout<N>(&mut self, net: &mut N)
    where
        N: RaftNetwork<Event = SM::Event>,
    {
        log::info!(
            "[{}] Heartbeat timeout, sending out heartbeats. D: {:#?}",
            self.id,
            self
        );
        if let RoleState::Leader { .. } = &self.role_state {
            self.update_peers(net, true);
            net.timer_reset(TimerKind::Heartbeat);
        }
    }

    pub(crate) fn handle_rpc<N>(
        &mut self,
        net: &mut N,
        peer_id: u64,
        msg: Msg<SM::Event>,
    ) -> Result<(), ()>
    where
        N: RaftNetwork<Event = SM::Event>,
    {
        if msg.term() > self.curr_term {
            log::info!(
                "[{}] Receive msg with larger term, become Follower. D: {:#?}",
                self.id,
                self
            );
            self.curr_term = msg.term();
            self.role_state = RoleState::Follower;
            net.timer_reset(TimerKind::Election);
        }
        match msg {
            Msg::AppendEntries(data) => self.append_entries(net, peer_id, data),
            Msg::AppendEntriesResponse(data) => self.append_entries_response(net, peer_id, data),
            Msg::RequestVote(data) => self.request_vote(net, peer_id, data),
            Msg::RequestVoteResponse(data) => self.request_vote_response(net, peer_id, data),
        }
    }

    fn append_entries<N>(
        &mut self,
        net: &mut N,
        peer_id: u64,
        append_entries: AppendEntries<SM::Event>,
    ) -> Result<(), ()>
    where
        N: RaftNetwork<Event = SM::Event>,
    {
        if let RoleState::Leader { .. } = self.role_state {
            return Err(());
        }
        let mut resp = AppendEntriesResponse {
            last_index: self.log.last_index(),
            term: self.curr_term,
            success: false,
        };

        if append_entries.term >= self.curr_term {
            match self.log.try_append(
                append_entries.prev_log_term,
                append_entries.prev_log_index,
                append_entries.entries,
            ) {
                Err(last_index) => {
                    log::info!("[{}] Append failed. D: {:#?}", self.id, self);
                    resp.last_index = last_index;
                }
                Ok(last_index) => {
                    if append_entries.leader_commit > self.commit_index {
                        self.commit_index = std::cmp::min(append_entries.leader_commit, last_index)
                    }
                    resp.success = true;
                    log::info!(
                        "[{}] Received data from leader, reset election timer. D: {:#?}",
                        self.id,
                        self
                    );

                    net.timer_reset(TimerKind::Election);
                }
            }
        }

        net.send(peer_id, Msg::AppendEntriesResponse(resp))
    }

    fn append_entries_response<N>(
        &mut self,
        net: &mut N,
        peer_id: u64,
        append_entries_response: AppendEntriesResponse,
    ) -> Result<(), ()>
    where
        N: RaftNetwork<Event = SM::Event>,
    {
        if let RoleState::Leader {
            next_indexes,
            match_indexes,
        } = &mut self.role_state
        {
            next_indexes.insert(peer_id, append_entries_response.last_index + 1);
            if append_entries_response.success {
                // success means log saved on peer
                match_indexes.insert(peer_id, append_entries_response.last_index);

                let quorum_match_index = utils::quorum_match_index(match_indexes.values());
                if let Some(entry) = self.log.at(quorum_match_index) {
                    if entry.term == self.curr_term {
                        self.commit_index = quorum_match_index;
                    }
                }
            } else {
                match_indexes.insert(peer_id, append_entries_response.last_index);
                // Retry to update peer
                // TODO: Logic for backoff, or set state to deal with replication instead
                let append = update_peer_request(
                    &self.log,
                    self.id,
                    self.curr_term,
                    self.commit_index,
                    &next_indexes,
                    peer_id,
                );
                return net.send(peer_id, Msg::AppendEntries(append));
            }
        }
        Ok(())
    }

    fn request_vote<N>(
        &mut self,
        net: &mut N,
        peer_id: u64,
        request_vote: RequestVote,
    ) -> Result<(), ()>
    where
        N: RaftNetwork<Event = SM::Event>,
    {
        let resp = if request_vote.term < self.curr_term {
            RequestVoteResponse {
                term: self.curr_term,
                vote_granted: false,
            }
        } else {
            match self.voted_for {
                Some(id) if id != request_vote.candidate_id => RequestVoteResponse {
                    term: self.curr_term,
                    vote_granted: false,
                },
                _ => {
                    let incoming = (request_vote.last_log_term, request_vote.last_log_index);
                    let current = (self.log.last_term(), self.log.last_index());
                    if incoming >= current {
                        RequestVoteResponse {
                            term: self.curr_term,
                            vote_granted: true,
                        }
                    } else {
                        RequestVoteResponse {
                            term: self.curr_term,
                            vote_granted: false,
                        }
                    }
                }
            }
        };
        if resp.vote_granted {
            net.timer_reset(TimerKind::Election);
        }
        net.send(peer_id, Msg::RequestVoteResponse(resp))
    }

    fn request_vote_response<N>(
        &mut self,
        net: &mut N,
        peer_id: u64,
        request_vote_response: RequestVoteResponse,
    ) -> Result<(), ()>
    where
        N: RaftNetwork<Event = SM::Event>,
    {
        if let RoleState::Candidate { votes } = &mut self.role_state {
            if request_vote_response.vote_granted {
                votes.insert(peer_id, true);

                let vote_count = votes.values().filter(|vote_granted| *vote_granted).count();
                let quorum = self.configuration.peers.len() / 2 + 1;
                if vote_count >= quorum {
                    let mut next_indexes = PeerIndexes::new();
                    let mut match_indexes = PeerIndexes::new();
                    for peer in self.configuration.peers.iter() {
                        next_indexes.insert(peer.id, self.log.last_index() + 1);
                        match_indexes.insert(peer.id, 0);
                    }
                    log::info!("[{}] Received quorum votes, become leader", self.id);
                    self.role_state = RoleState::Leader {
                        next_indexes,
                        match_indexes,
                    };
                    net.timer_reset(TimerKind::Heartbeat);
                }
            }
        }

        Ok(())
    }
}
