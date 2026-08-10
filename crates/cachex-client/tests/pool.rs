

use cachex_client::{CachexClient, ConnectionPool};
use cachex_core::protocol::{Command, Response};
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

#[tokio::test]
async fn pool_reuses_a_single_connection_per_node() {
    let node = start_node().await;
    let pool = ConnectionPool::new();

    for i in 0..100 {
        let command = Command::Set {
            key: format!("k:{i}"),
            value: format!("v:{i}").into_bytes(),
            ttl: None,
        };
        assert!(matches!(pool.command(&node, &command).await.unwrap(), Response::Ok));
    }

    assert_eq!(pool.tracked(), 1, "one node should map to one pooled connection");
}

#[tokio::test]
async fn client_round_trips_through_the_pool() {
    let node = start_node().await;
    let client = CachexClient::consistent(vec![node], 10);

    for i in 0..25 {
        let key = format!("pooled:{i}");
        let value = format!("value:{i}").into_bytes();
        client.set(&key, value.clone(), None).await.unwrap();
        assert_eq!(client.get(&key).await.unwrap(), Some(value));
    }
    assert!(client.delete("pooled:0").await.unwrap());
    assert_eq!(client.get("pooled:0").await.unwrap(), None);
}