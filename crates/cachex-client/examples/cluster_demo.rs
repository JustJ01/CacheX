

use cachex_client::connection::Connection;
use cachex_client::{CachexClient, Command};
use cachex_core::hashing::{ConsistentHasher, Router};

fn nodes_from_args() -> Vec<String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        vec![
            "127.0.0.1:7001".to_string(),
            "127.0.0.1:7002".to_string(),
            "127.0.0.1:7003".to_string(),
        ]
    } else {
        args
    }
}

#[tokio::main]
async fn main() {
    let nodes = nodes_from_args();
    println!("cluster: {nodes:?}");

    let hasher = ConsistentHasher::new(nodes.clone(), 100);
    let client = CachexClient::new(hasher);

    for i in 0..1000 {
        let key = format!("user:{i}");
        client.set(&key, format!("person-{i}").into_bytes(), None).await.unwrap();
    }

    let mut read_back = 0;
    for i in 0..1000 {
        let key = format!("user:{i}");
        if client.get(&key).await.unwrap().is_some() {
            read_back += 1;
        }
    }
    println!("wrote 1000 keys, read back {read_back}");

    let mut per_node = std::collections::HashMap::<String, usize>::new();
    for i in 0..1000 {
        *per_node
            .entry(client.router().primary(&format!("user:{i}")).to_string())
            .or_default() += 1;
    }
    for (node, count) in &per_node {
        println!("routed {count:4} keys to {node}");
    }

    for node in &nodes {
        let mut connection = Connection::connect(node).await.unwrap();
        if let Ok(cachex_client::Response::Info(text)) = connection
            .command(&Command::Info)
            .await
        {
            println!("{node}: {text}");
        }
    }
}