use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::Storage;
use crate::PeerConfig;
use crate::RaftNetwork;
use crate::StateMachine;
use crate::raft_state::Msg;
use crate::raft_state::RaftState;
use crate::raft_network::{SelectedAction, TimerKind};

// Node configurations & network
pub struct Node<SM, L, N>
where
    SM: StateMachine,
    L: Storage<Event = SM::Event>,
    N: RaftNetwork<Event = SM::Event>,
{
    id: u64,
    state: RaftState<SM, L>,
    network: N,
}

impl<SM, L, N> Node<SM, L, N>
where
    SM: StateMachine,
    L: Storage<Event = SM::Event>,
    N: RaftNetwork<Event = SM::Event>,
{
    pub fn new(
        node_id: u64,
        sm: SM,
        log: L,
        network: N,
        topology: Vec<PeerConfig>,
    ) -> Self {
        let state = RaftState::new(node_id, sm, log, topology);

        Node {
            id: node_id,
            state,
            network,
        }
    }

    pub fn start_loop(mut self) -> JoinHandle<()> {
        log::info!("[{}] Start node event loop", self.id);
        self.network.timer_reset(TimerKind::Election);
        let mut action_buf = Vec::new();

        thread::spawn(move || loop {
            self.tick(&mut action_buf, 100, Duration::from_millis(500));
        })
    }

    pub fn tick(&mut self, action_buf: &mut Vec<SelectedAction<SM::Event>>, max_actions: usize, timeout: Duration) -> bool {
        self.state.apply_committed();
        self.state.update_peers(&mut self.network, false);

        let timed_out = self.network.select_actions(action_buf, max_actions, timeout);
        log::info!("[{}] Node: {:#?}, Actions: {:#?}. Network: {:#?}", self.id, self.state, action_buf, self.network);

        for action in action_buf.drain(..) {
            match action {
                SelectedAction::Timer(timer_kind) => {
                    self.handle_timer(timer_kind);
                }
                SelectedAction::Client(event) => {
                    self.handle_client(event);
                }
                SelectedAction::Peer(id, msg) => {
                    self.handle_peer(id, msg);
                }
            };
        }

        timed_out
    }

    fn handle_client(&mut self, client_event: SM::Event) {
        self.state.propose(client_event);
    }

    fn handle_timer(&mut self, timer_kind: TimerKind) {
        use crate::raft_network::TimerKind::*;
        match timer_kind {
            Election => self.state.election_timeout(&mut self.network),
            Heartbeat => self.state.heartbeat_timeout(&mut self.network),
        }
    }

    fn handle_peer(&mut self, peer_id: u64, peer_msg: Msg<SM::Event>) {
        self.state.handle_rpc(&mut self.network, peer_id, peer_msg);
    }
}
