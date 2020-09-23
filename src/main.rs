use crossbeam_channel::{unbounded, Receiver, Sender};
use env_logger;
use log::info;
use rand::{thread_rng, Rng};
use rsm_raft::{Config, Node, Peer, StateMachine, VecLog};
use std::collections::HashMap;
use std::io;
use std::io::BufRead;

struct KVStore {
    data: HashMap<String, String>,
}

impl StateMachine for KVStore {
    type Snapshot = HashMap<String, String>;
    type Event = (String, String);

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

fn send_command(txs: &mut [Sender<(String, String)>], data: (String, String)) {
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
    let mut client_txs = Vec::new();
    let mut rng = thread_rng();
    let mut peers_per_node = Vec::new();
    for _i in 0..number_of_nodes {
        peers_per_node.push(Vec::new());
    }

    for i in 0..number_of_nodes {
        for j in i + 1..number_of_nodes {
            let (txi, rxi) = unbounded();
            let (txj, rxj) = unbounded();
            peers_per_node[i].push(Peer::new(j as u64, txi, rxj));
            peers_per_node[j].push(Peer::new(i as u64, txj, rxi));
        }
    }

    for i in 0..number_of_nodes {
        let config = Config::new(rng.gen_range(8_000_000, 10_000_000), 2_000_000);

        let (client_tx, client_rx) = unbounded();
        client_txs.push(client_tx);

        let sm = KVStore {
            data: HashMap::new(),
        };
        let log = VecLog::new();

        let node = Node::new(
            i as u64,
            config,
            sm,
            log,
            peers_per_node.remove(0),
            client_rx,
        );
        nodes.push(node);
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
    // Returns peer list for each node
    fn channels(num_of_nodes: u64) -> Vec<Vec<Peer>> {
        let mut result = vec![Vec::with_capacity(num_of_nodes); num_of_nodes];

        for i in 0..number_of_nodes {
            for j in i + 1..number_of_nodes {
                let (txi, rxi) = unbounded();
                let (txj, rxj) = unbounded();
                peers_per_node[i].push(Peer::new(j as u64, txi, rxj));
                peers_per_node[j].push(Peer::new(i as u64, txj, rxi));
            }
        }

        for per_node in result {
            per_node.remove(0);
        }

        result
    }

    #[test]
    fn vote_test() {
        let mut nodes = Vec::new();
        let mut client_txs = Vec::new();
        let mut rng = thread_rng();
        let mut peers_per_node = channels(number_of_nodes);

        for i in 0..number_of_nodes {
            let config = Config::new(rng.gen_range(8_000_000, 10_000_000), 2_000_000);

            let (client_tx, client_rx) = unbounded();
            client_txs.push(client_tx);

            let sm = KVStore {
                data: HashMap::new(),
            };
            let log = VecLog::new();

            let node = Node::new(
                i as u64,
                config,
                sm,
                log,
                peers_per_node.remove(0),
                client_rx,
            );
            nodes.push(node);
        }
        let node_threads = nodes.into_iter().map(Node::start_loop).collect::<Vec<_>>();
    }
}
