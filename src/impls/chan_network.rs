use std::time::Duration;
use std::time::Instant;
use std::{fmt::Debug, ops::Index, ops::Range};

use crossbeam::channel::Select;
use crossbeam::channel::{after, unbounded};
use crossbeam::channel::{Receiver, Sender};

use crate::Msg;
use crate::raft_network::{TimerKind, RaftNetwork, SelectedAction};

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
    peers: Vec<Peer<E>>,
    client_rx: Receiver<E>,
    timer: Option<Timer>
}

impl<E: Clone + Debug> Debug for ChanNetwork<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let timer_kind = match &self.timer {
            Some(t) => match t.timer_kind {
                TimerKind::Election => "election",
                TimerKind::Heartbeat => "heartbeat",
            },
            None => "none"
        };
        f.debug_struct("ChanNetwork")
            .field("election_timeout", &self.election_timeout)
            .field("heartbeat_timeout", &self.heartbeat_timeout)
            .field("timer", &timer_kind)
            .finish()
    }
}

impl<E: Clone + Debug> ChanNetwork<E> {
    pub fn new(election_timeout: u64, heartbeat_timeout: u64, peers: Vec<Peer<E>>, client_rx: Receiver<E>) -> Self {
        ChanNetwork {
            election_timeout,
            heartbeat_timeout,
            peers,
            client_rx,
            timer: None,
        }
    }

    pub fn cluster<R: rand::Rng>(rng: &mut R, election_timeout_range: Range<u64>, heartbeat_timeout: u64, nodes_count: usize) -> (Vec<ChanNetwork<E>>, Vec<Sender<E>>) {
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
            let election_timeout = rng.gen_range(election_timeout_range.start, election_timeout_range.end);
            let (client_tx, client_rx) = unbounded();

            client_txs.push(client_tx);
            networks.push(ChanNetwork::new(election_timeout, heartbeat_timeout, peers_per_node.remove(0), client_rx));
        }

        (networks, client_txs)
    }
}

impl<E: Clone + Debug> ChanNetwork<E> {
    fn iter(&self) -> impl Iterator<Item = &Peer<E>> {
        self.peers.iter()
    }

    fn len(&self) -> usize {
        self.peers.len()
    }

    fn find_by_id(&self, id: u64) -> Option<&Peer<E>> {
        self.peers.iter().find(|p| p.id == id)
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
        let duration = match timer_kind {
            TimerKind::Heartbeat => {
                Duration::from_millis(self.heartbeat_timeout)
            }
            TimerKind::Election => {
                Duration::from_millis(self.election_timeout)
            }
        };
        self.timer = Some(Timer {
            timer_kind,
            rx: after(duration),
        });
    }

    fn peer_ids(&self) -> Box<dyn Iterator<Item = u64> + '_> {
        Box::new(self.peers.iter().map(|p| p.id))
    }

    fn select_action(&mut self) -> SelectedAction<Self::Event> {
        let mut select = Select::new();

        for peer in self.peers.iter() {
            select.recv(&peer.rx);
        }
        let client_index = select.recv(&self.client_rx);

        if let Some(ref t) = self.timer {
            select.recv(&t.rx);
        }

        let selected = select.select();
        match selected.index() {
            i if i == client_index => {
                let client_event = selected.recv(&self.client_rx).unwrap();
                SelectedAction::Client(client_event)
            }
            i if i > client_index => {
                selected.recv(&self.timer.as_ref().unwrap().rx).unwrap();
                SelectedAction::Timer(self.timer.as_ref().unwrap().timer_kind)
            }
            i if i < self.peers.len() => {
                let peer = &self.peers[i];
                let peer_msg = selected.recv(&peer.rx).unwrap();
                SelectedAction::Peer(peer.id, peer_msg)
            }
            _ => panic!("Fail selection"),
        }
    }
}


