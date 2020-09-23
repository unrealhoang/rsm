use std::fmt::Debug;

pub trait StateMachine: Send + 'static {
    type Snapshot: Send + 'static;
    type Event: Send + Clone + Debug + 'static;

    // Empty state machine
    fn new() -> Self;
    fn from_snapshot(snapshot: Self::Snapshot) -> Self;
    fn execute(&mut self, event: &Self::Event);
    fn execute_batch<'a>(&'a mut self, events: impl Iterator<Item = &'a Self::Event>) {
        for event in events {
            self.execute(event);
        }
    }
}
