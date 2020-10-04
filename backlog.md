* [x] Refactor into trait
    - [x] ~State to `<SM, L, N>`? -> Reuse old code~
    - [x] Handle client message
    - [x] Handle peer message
    - [x] Handle timer
* [ ] Init ChanNetwork in main demo/integration test
    - [x] Impl RaftNetwork for ChanNetwork
    - [ ] Integration test
* [x] Split RaftState by Role
* [x] Handle term for each peer msg
* [x] Batch network action
* [x] Rename Log to Storage
* [x] Move HardState to Storage
* [ ] Move client from network to Node, add Node#request, Node#query
* [ ] Move StateMachine to Storage
* [ ] Introduce Output type for SM
* [ ] StateMachine batch processing
* [ ] StateMachine snapshotting
* [ ] Topology change
* [ ] Refactor to type based State FSM LeaderState / NodeState
* [ ] Proper error handling
* [ ] CoW snapshotting
    * https://docs.rs/ipc-channel/0.14.1/ipc_channel/
