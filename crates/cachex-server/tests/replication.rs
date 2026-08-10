

use cachex_client::connection::Connection;
use cachex_client::{CachexClient, Command, Response};
use cachex_core::config::HeartbeatConfig;
use cachex_core::hashing::{ConsistentHasher, Router};
use cachex_server::heartbeat::{Heartbeat, PeerStatus};
use cachex_server::internal::run_internal;
use cachex_server::metrics::Metrics;
use cachex_server::node::NodeContext;
use cachex_server::replication::Replicator;
use cachex_server::server;
use cachex_server::storage::CacheStore;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;

async fn start_cluster(n: usize, rf: u32) -> (Vec<String>, Vec<String>, Vec<Arc<Metrics>>) {
    let mut public = Vec::new();
    let mut public_listeners = Vec::new();
    for _ in 0..n {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        public.push(listener.local_addr().unwrap().to_string());
        public_listeners.push(listener);
    }

    let mut internal = Vec::new();
    let mut internal_listeners = Vec::new();
    for _ in 0..n {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        internal.push(listener.local_addr().unwrap().to_string());
        internal_listeners.push(listener);
    }

    let internal_map: HashMap<String, String> =
        public.iter().cloned().zip(internal.iter().cloned()).collect();
    let router = Arc::new(ConsistentHasher::new(public.clone(), 10));

    let mut public_listeners = public_listeners.into_iter();
    let mut internal_listeners = internal_listeners.into_iter();
    let mut metrics_vec = Vec::new();
    for i in 0..n {
        let store = Arc::new(CacheStore::new(1_000_000));
        let metrics = Arc::new(Metrics::new());
        let replicator = if rf > 1 {
            Some(Arc::new(Replicator::new(
                router.clone(),
                internal_map.clone(),
                rf,
            )))
        } else {
            None
        };
        let ctx = Arc::new(NodeContext {
            store: store.clone(),
            metrics: metrics.clone(),
            aof: None,
            router: router.clone(),
            self_address: public[i].clone(),
            replicator,
            heartbeat: None,
            replication_factor: rf,
        });
        let ctx_public = ctx.clone();
        let ctx_internal = ctx.clone();
        let public_listener = public_listeners.next().unwrap();
        let internal_listener = internal_listeners.next().unwrap();
        tokio::spawn(async move {
            let _ = server::run(public_listener, ctx_public).await;
        });
        tokio::spawn(async move {
            let _ = run_internal(internal_listener, ctx_internal).await;
        });
        metrics_vec.push(metrics.clone());
    }
    (public, internal, metrics_vec)
}

async fn node_get(addr: &str, key: &str) -> Option<Vec<u8>> {
    let mut connection = Connection::connect(addr).await.unwrap();
    match connection.command(&Command::Get { key: key.to_string() }).await.unwrap() {
        Response::Value(v) => Some(v),
        Response::NotFound => None,
        other => panic!("unexpected {other:?}"),
    }
}

#[tokio::test]
async fn set_reaches_replica_async() {
    let (public, _internal, _metrics) = start_cluster(2, 2).await;
    let ring = ConsistentHasher::new(public.clone(), 10);
    let client = CachexClient::new(ring);

    client.set("user:101", b"Jill".to_vec(), None).await.unwrap();
    let primary = client.router().primary("user:101").to_string();
    let replica = public.iter().find(|a| **a != primary).unwrap();

    assert_eq!(node_get(&primary, "user:101").await, Some(b"Jill".to_vec()));
    for _ in 0..40 {
        if node_get(replica, "user:101").await == Some(b"Jill".to_vec()) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("replica never received key");
}

#[tokio::test]
async fn delete_reaches_replica() {
    let (public, _internal, _metrics) = start_cluster(2, 2).await;
    let ring = ConsistentHasher::new(public.clone(), 10);
    let client = CachexClient::new(ring);

    client.set("k1", b"v1".to_vec(), None).await.unwrap();
    let primary = client.router().primary("k1").to_string();
    let replica = public.iter().find(|a| **a != primary).unwrap();

    for _ in 0..40 {
        if node_get(replica, "k1").await == Some(b"v1".to_vec()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(client.delete("k1").await.unwrap());
    for _ in 0..40 {
        if node_get(replica, "k1").await.is_none() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("replica never saw the delete");
}

#[tokio::test]
async fn primary_has_replica_count_metric() {
    let (public, _internal, metrics) = start_cluster(2, 2).await;
    let ring = ConsistentHasher::new(public.clone(), 10);
    let client = CachexClient::new(ring);

    client.set("metric:key", b"m".to_vec(), None).await.unwrap();
    let primary = client.router().primary("metric:key").to_string();
    let idx = public.iter().position(|a| a == &primary).unwrap();
    let m = &metrics[idx];
    assert!(m.replication_sent.load(Ordering::Relaxed) >= 1);
    assert_eq!(m.replication_failed.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn heartbeat_marks_live_peer_alive() {
    let (public, internal, _) = start_cluster(1, 1).await;
    let map: HashMap<String, String> =
        public.iter().cloned().zip(internal.iter().cloned()).collect();
    let hb = Arc::new(Heartbeat::new(
        "self",
        &[public[0].clone()],
        map,
        HeartbeatConfig {
            interval_secs: 1,
            timeout_ms: 200,
            miss_threshold: 2,
        },
    ));
    hb.tick().await;
    assert_eq!(hb.status_of(&public[0]), Some(PeerStatus::Alive));
}

#[tokio::test]
async fn heartbeat_detects_dead_peer() {
    
    
    let dead_internal = "127.0.0.1:1";
    let map: HashMap<String, String> =
        HashMap::from([("dead:7001".to_string(), dead_internal.to_string())]);
    let hb = Arc::new(Heartbeat::new(
        "self",
        &["dead:7001".to_string()],
        map,
        HeartbeatConfig {
            interval_secs: 1,
            timeout_ms: 100,
            miss_threshold: 3,
        },
    ));

    hb.tick().await;
    assert_eq!(hb.status_of("dead:7001"), Some(PeerStatus::Suspected));
    hb.tick().await;
    hb.tick().await;
    assert_eq!(hb.status_of("dead:7001"), Some(PeerStatus::Failed));
    let (_, _, failed) = hb.summary();
    assert_eq!(failed, 1);
}