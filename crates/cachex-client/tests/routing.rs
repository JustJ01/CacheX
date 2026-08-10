

use cachex_client::connection::Connection;
use cachex_client::{CachexClient, Command, Response};
use cachex_core::hashing::{ConsistentHasher, ModuloHasher, Router};
use cachex_server::{metrics::Metrics, node::NodeContext, server, storage::CacheStore};
use std::sync::Arc;
use tokio::net::TcpListener;

async fn start_node() -> String {
    let store = Arc::new(CacheStore::new(1_000_000));
    let metrics = Arc::new(Metrics::new());
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let ctx = NodeContext::standalone(store, metrics, None, address.clone());
    tokio::spawn(async move {
        let _ = server::run(listener, ctx).await;
    });
    address
}

async fn nodes() -> Vec<String> {
    let mut out = Vec::new();
    for _ in 0..3 {
        out.push(start_node().await);
    }
    out
}

async fn node_holds(node: &str, key: &str) -> Option<Vec<u8>> {
    let mut connection = Connection::connect(node).await.unwrap();
    match connection.command(&Command::Get { key: key.to_string() }).await.unwrap() {
        Response::Value(value) => Some(value),
        Response::NotFound => None,
        other => panic!("unexpected response {other:?}"),
    }
}

#[tokio::test]
async fn consistent_client_routes_key_to_its_primary_only() {
    let nodes = nodes().await;
    let ring = ConsistentHasher::new(nodes.clone(), 10);
    let client = CachexClient::new(ring);

    client.set("user:101", b"Jill".to_vec(), None).await.unwrap();

    let primary = client.router().primary("user:101").to_string();
    assert_eq!(node_holds(&primary, "user:101").await, Some(b"Jill".to_vec()));
    for node in nodes.iter().filter(|n| **n != primary) {
        assert_eq!(
            node_holds(node, "user:101").await,
            None,
            "non-primary node {node} must not hold the key"
        );
    }
}

#[tokio::test]
async fn modulo_client_routes_key_to_its_primary_only() {
    let nodes = nodes().await;
    let client = CachexClient::new(ModuloHasher::new(nodes.clone()));

    client.set("product:52", b"Laptop".to_vec(), None).await.unwrap();

    let primary = client.router().primary("product:52").to_string();
    assert_eq!(node_holds(&primary, "product:52").await, Some(b"Laptop".to_vec()));
    for node in nodes.iter().filter(|n| **n != primary) {
        assert_eq!(node_holds(node, "product:52").await, None);
    }
}

#[tokio::test]
async fn client_reads_back_every_key_it_writes() {
    let nodes = nodes().await;
    let client = CachexClient::consistent(nodes.clone(), 10);

    for i in 0..50 {
        let key = format!("key:{i}");
        let value = format!("value:{i}").into_bytes();
        client.set(&key, value.clone(), None).await.unwrap();
        assert_eq!(client.get(&key).await.unwrap(), Some(value));
    }
}

#[tokio::test]
async fn delete_removes_key_from_primary() {
    let nodes = nodes().await;
    let client = CachexClient::consistent(nodes.clone(), 10);

    client.set("session:abc", b"xyz123".to_vec(), None).await.unwrap();
    assert!(client.get("session:abc").await.unwrap().is_some());
    assert!(client.delete("session:abc").await.unwrap());
    assert_eq!(client.get("session:abc").await.unwrap(), None);
    assert!(!client.delete("session:abc").await.unwrap(), "second delete finds nothing");
}

#[tokio::test]
async fn consistent_router_returns_distinct_replicas() {
    let nodes = nodes().await;
    let ring = ConsistentHasher::new(nodes.clone(), 10);

    for i in 0..100 {
        let key = format!("key:{i}");
        let primary = ring.primary(&key).to_string();
        let replicas = ring.replicas(&key, 2);
        assert_eq!(replicas.len(), 2);
        assert!(replicas.iter().all(|r| *r != primary));
        assert_ne!(replicas[0], replicas[1]);
    }
}