use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;
use std::{fmt::Debug, ops::Index, ops::Range};

use crossbeam::channel::Select;
use crossbeam::channel::{after, unbounded};
use crossbeam::channel::{Receiver, Sender};

use crate::raft_network::{RaftNetwork, SelectedAction, TimerKind};
use crate::Msg;

// Channels to communicate with peers
pub struct Peer<E: Clone + Debug> {
    id: u64,
    tx: Sender<Msg<E>>,
    rx: Receiver<Msg<E>>,
}

impl<E: Clone + Debug> Peer<E> {
    pub fn new(id: u64, tx: Sender<Msg<E>>, rx: Receiver<Msg<E>>) -> Self {
        Peer { id, tx, rx }
    }
}

struct Timer {
    rx: Receiver<Instant>,
    timer_kind: TimerKind,
}

pub struct ChanNetwork<E: Clone + Debug> {
    election_timeout: u64,
    heartbeat_timeout: u64,
    controlled_timer: bool,
    peers: Vec<Peer<E>>,
    client_rx: Receiver<E>,
    timer: Option<Timer>,
}

impl<E: Clone + Debug> Debug for ChanNetwork<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let timer_kind = match &self.timer {
            Some(t) => match t.timer_kind {
                TimerKind::Election => "election",
                TimerKind::Heartbeat => "heartbeat",
            },
            None => "none",
        };
        f.debug_struct("ChanNetwork")
            .field("election_timeout", &self.election_timeout)
            .field("heartbeat_timeout", &self.heartbeat_timeout)
            .field("timer", &timer_kind)
            .finish()
    }
}

impl<E: Clone + Debug> ChanNetwork<E> {
    pub fn new(
        election_timeout: u64,
        heartbeat_timeout: u64,
        peers: Vec<Peer<E>>,
        client_rx: Receiver<E>,
        controlled_timer: bool,
    ) -> Self {
        ChanNetwork {
            election_timeout,
            heartbeat_timeout,
            peers,
            client_rx,
            controlled_timer,
            timer: None,
        }
    }

    pub fn cluster<R: rand::Rng>(
        rng: &mut R,
        election_timeout_range: Range<u64>,
        heartbeat_timeout: u64,
        nodes_count: usize,
        controlled_timer: bool,
    ) -> (Vec<ChanNetwork<E>>, Vec<Sender<E>>) {
        let mut peers_per_node = Vec::new();

        for _i in 0..nodes_count {
            peers_per_node.push(Vec::new());
        }

        for i in 0..nodes_count {
            for j in i + 1..nodes_count {
                let (txi, rxi) = unbounded();
                let (txj, rxj) = unbounded();
                peers_per_node[i].push(Peer::new(j as u64, txi, rxj));
                peers_per_node[j].push(Peer::new(i as u64, txj, rxi));
            }
        }

        let mut client_txs = Vec::new();
        let mut networks = Vec::new();

        for i in 0..nodes_count {
            let election_timeout =
                rng.gen_range(election_timeout_range.start, election_timeout_range.end);
            let (client_tx, client_rx) = unbounded();

            client_txs.push(client_tx);
            networks.push(ChanNetwork::new(
                election_timeout,
                heartbeat_timeout,
                peers_per_node.remove(0),
                client_rx,
                controlled_timer,
            ));
        }

        (networks, client_txs)
    }

    fn select_action(&mut self, wait_time: Duration) -> Option<SelectedAction<E>> {
        let mut select = Select::new();

        for peer in self.peers.iter() {
            select.recv(&peer.rx);
        }
        let client_index = select.recv(&self.client_rx);

        if let Some(ref t) = self.timer {
            select.recv(&t.rx);
        }

        let selected = select.select_timeout(wait_time).ok()?;
        match selected.index() {
            i if i == client_index => {
                let client_event = selected.recv(&self.client_rx).unwrap();
                Some(SelectedAction::Client(client_event))
            }
            i if i > client_index => {
                selected.recv(&self.timer.as_ref().unwrap().rx).unwrap();
                Some(SelectedAction::Timer(
                    self.timer.as_ref().unwrap().timer_kind,
                ))
            }
            i if i < self.peers.len() => {
                let peer = &self.peers[i];
                let peer_msg = selected.recv(&peer.rx).unwrap();
                Some(SelectedAction::Peer(peer.id, peer_msg))
            }
            _ => panic!("Fail selection"),
        }
    }

    fn iter(&self) -> impl Iterator<Item = &Peer<E>> {
        self.peers.iter()
    }

    fn len(&self) -> usize {
        self.peers.len()
    }

    fn find_by_id(&self, id: u64) -> Option<&Peer<E>> {
        self.peers.iter().find(|p| p.id == id)
    }

    pub fn trigger_timer(&mut self) {
        if !self.controlled_timer {
            return;
        }

        self.timer
            .as_mut()
            .map(|t| t.rx = after(Duration::from_secs(0)));
    }
}

impl<E: Clone + Debug> Index<usize> for ChanNetwork<E> {
    type Output = Peer<E>;
    fn index(&self, index: usize) -> &Self::Output {
        &self.peers[index]
    }
}

impl<E: Clone + Debug + Send + 'static> RaftNetwork for ChanNetwork<E> {
    type Event = E;

    fn send(&mut self, peer_id: u64, msg: Msg<Self::Event>) -> Result<(), ()> {
        let p = self.find_by_id(peer_id).ok_or(())?;
        p.tx.send(msg).map_err(|_| ())
    }

    fn timer_reset(&mut self, timer_kind: TimerKind) {
        let duration = if self.controlled_timer {
            match timer_kind {
                TimerKind::Heartbeat => Duration::from_millis(self.heartbeat_timeout),
                TimerKind::Election => Duration::from_millis(self.election_timeout),
            }
        } else {
            // practically forever
            Duration::from_secs(1000_000_000)
        };
        self.timer = Some(Timer {
            timer_kind,
            rx: after(duration),
        });
    }

    fn select_actions(
        &mut self,
        buf: &mut Vec<SelectedAction<Self::Event>>,
        max_actions: usize,
        max_wait_time: Duration,
    ) -> bool {
        let duration_zero = Duration::new(0, 0);

        let mut timeout = max_wait_time;
        for _ in 0..max_actions {
            let instant = Instant::now();
            if let Some(action) = self.select_action(timeout) {
                buf.push(action);
                timeout = timeout
                    .checked_sub(instant.elapsed())
                    .unwrap_or(duration_zero);
            } else {
                return true;
            }
        }

        false
    }
}

impl<E: Clone + Debug + Send + 'static> RaftNetwork for Arc<Mutex<ChanNetwork<E>>> {
    type Event = E;

    fn send(&mut self, peer_id: u64, msg: Msg<Self::Event>) -> Result<(), ()> {
        self.lock().unwrap().send(peer_id, msg)
    }

    fn timer_reset(&mut self, timer_kind: TimerKind) {
        self.lock().unwrap().timer_reset(timer_kind)
    }

    fn select_actions(
        &mut self,
        buf: &mut Vec<SelectedAction<Self::Event>>,
        max_actions: usize,
        max_wait_time: Duration,
    ) -> bool {
        self.lock()
            .unwrap()
            .select_actions(buf, max_actions, max_wait_time)
    }
}
