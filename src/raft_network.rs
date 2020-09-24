use std::time::Duration;
use std::time::Instant;
use std::{fmt::Debug, ops::Index};

use crossbeam_channel::Select;
use crossbeam_channel::after;
use crossbeam_channel::{Receiver, Sender};

use crate::Msg;

#[derive(Debug)]
pub enum SelectedAction<E: Clone + Debug> {
    Client(E),
    Peer(u64, Msg<E>),
    Timer(TimerKind),
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum TimerKind {
    Heartbeat,
    Election,
}

pub trait RaftNetwork: Send + Debug + 'static {
    type Event: Clone + Debug;

    fn send(&mut self, peer_id: u64, msg: Msg<Self::Event>) -> Result<(), ()>;
    fn send_all<I>(&mut self, targets: I) -> Result<(), ()>
    where
        I: Iterator<Item = (u64, Msg<Self::Event>)>
    {
        for (peer_id, msg) in targets {
            self.send(peer_id, msg)?;
        }
        Ok(())
    }

    fn timer_reset(&mut self, timer_kind: TimerKind);

    fn peer_ids(&self) -> Box<dyn Iterator<Item = u64> + '_>;

    fn select_action(&mut self) -> SelectedAction<Self::Event>;
}

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
                Duration::from_micros(self.heartbeat_timeout)
            }
            TimerKind::Election => {
                Duration::from_micros(self.election_timeout)
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


