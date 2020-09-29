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
* [ ] Introduce Output type for SM
* [ ] StateMachine batch processing
* [ ] StateMachine snapshotting
* [ ] Topology change
* [ ] Refactor to LeaderState / NodeState
* [ ] Proper error handling
