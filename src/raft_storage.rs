use std::fmt::Debug;

#[derive(Clone, Debug)]
pub struct LogEntry<E> {
    pub index: u64,
    pub term: u64,
    pub data: E,
}

pub trait Storage: Send + 'static {
    type Event: Clone;

    fn at(&self, index: u64) -> Option<&LogEntry<Self::Event>>;
    fn last_term(&self) -> u64;
    fn last_index(&self) -> u64;
    fn try_append(
        &mut self,
        prev_term: u64,
        prev_index: u64,
        entries: Vec<LogEntry<Self::Event>>,
    ) -> Result<u64, u64>;
    fn slice(&self, from_index: u64, to_index: u64) -> &[LogEntry<Self::Event>];
    fn slice_to_end(&self, from_index: u64) -> &[LogEntry<Self::Event>];
    fn current_term(&self) -> u64;
    fn voted_for(&self) -> Option<u64>;
    fn set_current_term(&mut self, current_term: u64);
    fn set_voted_for(&mut self, voted_for: Option<u64>);

    fn propose(&mut self, term: u64, event: Self::Event);
}
