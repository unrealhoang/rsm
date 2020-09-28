use std::fmt::Debug;
use std::time::Duration;

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

    fn select_actions(&mut self, buf: &mut Vec<SelectedAction<Self::Event>>, max_action: usize, max_wait_time: Duration) -> bool;
}

