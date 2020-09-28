use crossbeam::channel::Sender;
use env_logger;
use log::info;
use rand::thread_rng;
use rsm_raft::{
    Node, StateMachine,
    raft_log::VecLog,
};
use rsm_raft::impls::ChanNetwork;
use std::collections::HashMap;
use std::io;
use std::io::BufRead;

struct KVStore {
    data: HashMap<String, String>,
}

type Event = (String, String);
impl StateMachine for KVStore {
    type Snapshot = HashMap<String, String>;
    type Event = Event;

    // Empty state machine
    fn new() -> Self {
        KVStore {
            data: HashMap::new(),
        }
    }
    fn from_snapshot(snapshot: Self::Snapshot) -> Self {
        KVStore { data: snapshot }
    }
    fn execute(&mut self, event: &Self::Event) {
        self.data.insert(event.0.clone(), event.1.clone());
    }
}

fn send_command(txs: &mut [Sender<Event>], data: Event) {
    info!("Send command to cluster");
    for tx in txs {
        tx.send(data.clone()).unwrap();
    }
}

fn main() {
    env_logger::init();
    let number_of_nodes = std::env::args()
        .skip(1)
        .next()
        .expect("Number of nodes missing")
        .parse::<usize>()
        .expect("Parse error");
    let mut nodes = Vec::new();
    let mut rng = thread_rng();
    let (networks, mut client_txs) = ChanNetwork::cluster(&mut rng, 15_000..18_000, 2_000, number_of_nodes);

    let mut i = 0;
    for network in networks {
        let sm = KVStore {
            data: HashMap::new(),
        };
        let log = VecLog::new();

        let node = Node::new(
            i as u64,
            sm,
            log,
            network,
        );
        nodes.push(node);
        i += 1;
    }
    let node_threads = nodes.into_iter().map(Node::start_loop).collect::<Vec<_>>();

    let mut line = String::new();
    let stdin = io::stdin();
    let mut reader = io::BufReader::new(stdin);
    loop {
        reader.read_line(&mut line).unwrap();
        if line == "EXIT" {
            break;
        }
        if line.starts_with("SET") {
            let mut parts = line.split_whitespace();
            parts.next();
            if let Some(content) = parts.next() {
                let mut kv = content.splitn(2, "=");
                let key = kv.next();
                let value = kv.next();

                match (key, value) {
                    (Some(k), Some(v)) => {
                        send_command(&mut client_txs, (k.to_owned(), v.to_owned()));
                    }
                    _ => (),
                }
            }
        }
        line.clear();
    }

    for t in node_threads {
        t.join().unwrap();
    }
}

#[cfg(test)]
mod tests {
    use rsm_raft::{
        Node, StateMachine,
        raft_log::VecLog,
    };
    use rsm_raft::impls::ChanNetwork;
    use std::collections::HashMap;
    use crossbeam::channel::Sender;
    use rand::{thread_rng};

    struct KVStore {
        data: HashMap<String, String>,
    }

    type Event = (String, String);
    impl StateMachine for KVStore {
        type Snapshot = HashMap<String, String>;
        type Event = Event;

        // Empty state machine
        fn new() -> Self {
            KVStore {
                data: HashMap::new(),
            }
        }
        fn from_snapshot(snapshot: Self::Snapshot) -> Self {
            KVStore { data: snapshot }
        }

        fn execute(&mut self, event: &Self::Event) {
            self.data.insert(event.0.clone(), event.1.clone());
        }
    }

    fn send_command(txs: &mut [Sender<Event>], data: Event) {
        log::info!("Send command to cluster");
        for tx in txs {
            tx.send(data.clone()).unwrap();
        }
    }

    #[test]
    fn vote_test() {
        let mut nodes = Vec::new();
        let mut rng = thread_rng();
        let number_of_nodes = 3;
        let (networks, mut client_txs) = ChanNetwork::cluster(&mut rng, 15_000..18_000, 2_000, number_of_nodes);

        let mut i = 0;
        for network in networks {
            let sm = KVStore {
                data: HashMap::new(),
            };
            let log = VecLog::new();

            let node = Node::new(
                i as u64,
                sm,
                log,
                network,
            );
            nodes.push(node);
            i += 1;
        }
        let node_threads = nodes.into_iter().map(Node::start_loop).collect::<Vec<_>>();
    }
}
