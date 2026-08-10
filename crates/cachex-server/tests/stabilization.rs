

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
use std::future::Future;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

const CLIENT_COUNT: usize = 50;
const VNODES: usize = 10;

struct Node {
    public: String,
    metrics: Arc<Metrics>,
    public_handle: JoinHandle<()>,
    internal_handle: JoinHandle<()>,
}

impl Node {
    fn kill_public(&self) {
        self.public_handle.abort();
    }

    fn kill_internal(&self) {
        self.internal_handle.abort();
    }
}

async fn start_node(
    public: String,
    public_listener: TcpListener,
    _internal: String,
    internal_listener: TcpListener,
    router: Arc<ConsistentHasher>,
    internal_map: HashMap<String, String>,
    rf: u32,
) -> Node {
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
        self_address: public.clone(),
        replicator,
        heartbeat: None,
        replication_factor: rf,
    });
    let ctx_public = ctx.clone();
    let ctx_internal = ctx.clone();
    let public_handle = tokio::spawn(async move {
        let _ = server::run(public_listener, ctx_public).await;
    });
    let internal_handle = tokio::spawn(async move {
        let _ = run_internal(internal_listener, ctx_internal).await;
    });
    Node {
        public,
        metrics,
        public_handle,
        internal_handle,
    }
}

async fn start_cluster(n: usize, rf: u32) -> Vec<Node> {
    let mut bound = Vec::new();
    for _ in 0..n {
        let p = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let i = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        bound.push((
            p.local_addr().unwrap().to_string(),
            p,
            i.local_addr().unwrap().to_string(),
            i,
        ));
    }
    let publics: Vec<String> = bound.iter().map(|(p, _, _, _)| p.clone()).collect();
    let internals: Vec<String> = bound.iter().map(|(_, _, i, _)| i.clone()).collect();
    let internal_map: HashMap<String, String> =
        publics.iter().cloned().zip(internals.iter().cloned()).collect();
    let router = Arc::new(ConsistentHasher::new(publics.clone(), VNODES));

    let mut nodes = Vec::new();
    for (public, public_listener, internal, internal_listener) in bound {
        nodes.push(
            start_node(
                public,
                public_listener,
                internal,
                internal_listener,
                router.clone(),
                internal_map.clone(),
                rf,
            )
            .await,
        );
    }
    nodes
}

async fn client(nodes: &[Node]) -> CachexClient<ConsistentHasher> {
    CachexClient::consistent(nodes.iter().map(|n| n.public.clone()).collect(), VNODES)
}

async fn node_get(addr: &str, key: &str) -> Option<Vec<u8>> {
    let mut connection = Connection::connect(addr).await.unwrap();
    match connection.command(&Command::Get { key: key.to_string() }).await.unwrap() {
        Response::Value(v) => Some(v),
        Response::NotFound => None,
        other => panic!("unexpected {other:?}"),
    }
}

async fn wait_until<Fut, F>(mut cond: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    for _ in 0..100 {
        if cond().await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("condition never became true in time");
}

fn keys_primary_on(router: &impl Router, node_addr: &str, count: usize) -> Vec<String> {
    let mut out = Vec::new();
    for i in 0..100_000 {
        let key = format!("stabilization:{i}");
        if router.primary(&key) == node_addr {
            out.push(key);
            if out.len() == count {
                break;
            }
        }
    }
    assert_eq!(out.len(), count, "could not find {count} keys primary on {node_addr}");
    out
}

#[tokio::test]
async fn t1_normal_replication() {
    let nodes = start_cluster(2, 2).await;
    let client = client(&nodes).await;

    client.set("user:101", b"Jill".to_vec(), None).await.unwrap();

    let primary = client.router().primary("user:101").to_string();
    let replica = nodes.iter().map(|n| n.public.clone()).find(|a| *a != primary).unwrap();

    
    assert_eq!(node_get(&primary, "user:101").await, Some(b"Jill".to_vec()));
    wait_until(async || node_get(&replica, "user:101").await == Some(b"Jill".to_vec())).await;
}

#[tokio::test]
async fn t2_replica_failure_does_not_block_primary() {
    let nodes = start_cluster(2, 2).await;
    let client = client(&nodes).await;

    
    let keys = keys_primary_on(client.router(), &nodes[0].public, 1);
    let key = &keys[0];

    
    
    nodes[1].kill_internal();
    nodes[1].kill_public();

    
    client.set(key, b"v2".to_vec(), None).await.expect("primary must accept writes");
    assert_eq!(node_get(&nodes[0].public, key).await, Some(b"v2".to_vec()));

    
    let mut failed = 0u64;
    for _ in 0..100 {
        failed = nodes[0].metrics.replication_failed.load(Ordering::Relaxed);
        if failed >= 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(failed >= 1, "replication_failed stayed at {failed}");
}

#[tokio::test]
async fn t3_failure_detection_suspected_then_failed() {
    let (addr, listener) = bind_internal().await;
    let store = Arc::new(CacheStore::new(1_000_000));
    let metrics = Arc::new(Metrics::new());
    let ctx = NodeContext::standalone(store.clone(), metrics.clone(), None, "peer".into());
    let handle = tokio::spawn(async move {
        let _ = run_internal(listener, ctx).await;
    });

    let hb = heartbeat_for(&addr, 2);
    hb.tick().await;
    assert_eq!(hb.status_of("peer"), Some(PeerStatus::Alive));

    
    handle.abort();

    
    hb.tick().await;
    assert_eq!(hb.status_of("peer"), Some(PeerStatus::Suspected));

    
    hb.tick().await;
    assert_eq!(hb.status_of("peer"), Some(PeerStatus::Failed));
    let (alive, suspected, failed) = hb.summary();
    assert_eq!((alive, suspected, failed), (0, 0, 1));
}

#[tokio::test]
async fn t4_recovery_failed_to_alive() {
    let port = pick_free_port().await;
    let store = Arc::new(CacheStore::new(1_000_000));
    let metrics = Arc::new(Metrics::new());

    
    let (addr, listener) = bind_on(port).await;
    let ctx = NodeContext::standalone(store.clone(), metrics.clone(), None, "peer".into());
    let handle = tokio::spawn(async move {
        let _ = run_internal(listener, ctx).await;
    });

    let hb = heartbeat_for(&addr, 2);
    hb.tick().await;
    assert_eq!(hb.status_of("peer"), Some(PeerStatus::Alive));

    handle.abort();
    hb.tick().await; 
    hb.tick().await; 
    assert_eq!(hb.status_of("peer"), Some(PeerStatus::Failed));
    assert_eq!(hb.last_detection_ns.load(Ordering::Relaxed), 0, "not recovered yet");

    
    let (restarted_addr, listener) = bind_on(port).await;
    assert_eq!(restarted_addr, addr);
    let ctx = NodeContext::standalone(store.clone(), metrics.clone(), None, "peer".into());
    let restarted = tokio::spawn(async move {
        let _ = run_internal(listener, ctx).await;
    });

    
    
    hb.tick().await;
    assert_eq!(hb.status_of("peer"), Some(PeerStatus::Alive));
    assert!(
        hb.last_detection_ns.load(Ordering::Relaxed) > 0,
        "recovery should be recorded once the peer returns"
    );
    let (alive, suspected, failed) = hb.summary();
    assert_eq!((alive, suspected, failed), (1, 0, 0));

    restarted.abort();
}

#[tokio::test]
async fn t5_multiple_writes_sequence() {
    let nodes = start_cluster(2, 2).await;
    let client = client(&nodes).await;

    
    let keys = keys_primary_on(client.router(), &nodes[0].public, 4);
    let (a, b, c, d) = (keys[0].as_str(), keys[1].as_str(), keys[2].as_str(), keys[3].as_str());
    let replica = nodes.iter().map(|n| n.public.clone()).find(|p| *p != nodes[0].public).unwrap();

    
    client.set(a, b"v-A".to_vec(), None).await.unwrap();
    client.set(b, b"v-B".to_vec(), None).await.unwrap();
    client.set(c, b"v-C".to_vec(), None).await.unwrap();
    assert!(client.delete(b).await.unwrap(), "DELETE B should succeed");
    client.set(d, b"v-D".to_vec(), None).await.unwrap();

    
    wait_until(async || node_get(&replica, a).await == Some(b"v-A".to_vec())).await;
    wait_until(async || node_get(&replica, c).await == Some(b"v-C".to_vec())).await;
    wait_until(async || node_get(&replica, d).await == Some(b"v-D".to_vec())).await;
    wait_until(async || node_get(&replica, b).await.is_none()).await;

    
    assert_eq!(node_get(&nodes[0].public, a).await, Some(b"v-A".to_vec()));
    assert_eq!(node_get(&nodes[0].public, b).await, None);
    assert_eq!(node_get(&nodes[0].public, c).await, Some(b"v-C".to_vec()));
    assert_eq!(node_get(&nodes[0].public, d).await, Some(b"v-D".to_vec()));
}

#[tokio::test]
async fn t6_concurrent_writes_under_load() {
    let nodes = start_cluster(3, 2).await;
    let client = Arc::new(client(&nodes).await);

    let writes_per_client = 5;
    let total_writes = (CLIENT_COUNT * writes_per_client) as u64;

    
    
    tokio::time::timeout(Duration::from_secs(30), async {
        let mut handles = Vec::new();
        for c in 0..CLIENT_COUNT {
            let client = client.clone();
            handles.push(tokio::spawn(async move {
                for i in 0..writes_per_client {
                    let key = format!("load:{c}:{i}");
                    let value = format!("value:{c}:{i}").into_bytes();
                    client.set(&key, value, None).await.unwrap();
                }
            }));
        }
        for handle in handles {
            handle.await.unwrap();
        }
    })
    .await
    .expect("concurrent writes hung or deadlocked");

    
    for c in 0..CLIENT_COUNT {
        for i in 0..writes_per_client {
            let key = format!("load:{c}:{i}");
            let expected = format!("value:{c}:{i}").into_bytes();
            assert_eq!(
                client.get(&key).await.unwrap(),
                Some(expected),
                "key {key} inconsistent on primary"
            );
        }
    }

    
    let mut received = 0u64;
    for _ in 0..100 {
        received = nodes
            .iter()
            .map(|n| n.metrics.replication_received.load(Ordering::Relaxed))
            .sum();
        if received >= total_writes {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        received >= total_writes,
        "replication converged to only {received}/{total_writes} received"
    );

    
    for node in &nodes {
        assert_eq!(
            node.metrics.replication_failed.load(Ordering::Relaxed),
            0,
            "unexpected replication failure on {}",
            node.public
        );
    }

    
    
    for c in (0..CLIENT_COUNT).step_by(7) {
        for i in 0..writes_per_client {
            let key = format!("load:{c}:{i}");
            let primary = client.router().primary(&key).to_string();
            let replica = client.router().replicas(&key, 1)[0].to_string();
            wait_until(async || node_get(&primary, &key).await.is_some()).await;
            wait_until(async || node_get(&replica, &key).await.is_some()).await;
        }
    }
}

fn heartbeat_for(internal_addr: &str, miss_threshold: u32) -> Arc<Heartbeat> {
    let map: HashMap<String, String> =
        HashMap::from([("peer".to_string(), internal_addr.to_string())]);
    Arc::new(Heartbeat::new(
        "self",
        &["peer".to_string()],
        map,
        HeartbeatConfig {
            interval_secs: 1,
            timeout_ms: 200,
            miss_threshold,
        },
    ))
}

async fn pick_free_port() -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

async fn bind_internal() -> (String, TcpListener) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    (listener.local_addr().unwrap().to_string(), listener)
}

async fn bind_on(port: u16) -> (String, TcpListener) {
    for _ in 0..100 {
        if let Ok(listener) = TcpListener::bind(("127.0.0.1", port)).await {
            return (listener.local_addr().unwrap().to_string(), listener);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("could not rebind port {port}");
}