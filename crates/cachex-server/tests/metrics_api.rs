

use cachex_client::connection::Connection;
use cachex_client::{CachexClient, Command, Response};
use cachex_core::config::HeartbeatConfig;
use cachex_core::hashing::{ConsistentHasher, Router};
use cachex_server::heartbeat::Heartbeat;
use cachex_server::internal::run_internal;
use cachex_server::metrics::Metrics;
use cachex_server::metrics_http;
use cachex_server::node::NodeContext;
use cachex_server::replication::Replicator;
use cachex_server::server;
use cachex_server::storage::CacheStore;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const VNODES: usize = 10;

struct Node {
    public: String,
    metrics_addr: String,
}

async fn start_standalone() -> (String, String, Arc<Metrics>) {
    let public = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let metrics_listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let public_addr = public.local_addr().unwrap().to_string();
    let metrics_addr = metrics_listener.local_addr().unwrap().to_string();

    let store = Arc::new(CacheStore::new(1_000_000));
    let metrics = Arc::new(Metrics::new());
    let ctx = NodeContext::standalone(store, metrics.clone(), None, public_addr.clone());
    let ctx_public = ctx.clone();
    let ctx_metrics = ctx.clone();
    tokio::spawn(async move {
        let _ = server::run(public, ctx_public).await;
    });
    tokio::spawn(async move {
        let _ = metrics_http::serve(metrics_listener, ctx_metrics).await;
    });
    (public_addr, metrics_addr, metrics)
}

async fn start_cluster_with_metrics(n: usize, rf: u32) -> Vec<Node> {
    let mut bound = Vec::new();
    for _ in 0..n {
        let p = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let i = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let m = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        bound.push((
            p.local_addr().unwrap().to_string(),
            p,
            i.local_addr().unwrap().to_string(),
            i,
            m.local_addr().unwrap().to_string(),
            m,
        ));
    }
    let publics: Vec<String> = bound.iter().map(|(p, _, _, _, _, _)| p.clone()).collect();
    let internals: Vec<String> = bound.iter().map(|(_, _, i, _, _, _)| i.clone()).collect();
    let internal_map: HashMap<String, String> =
        publics.iter().cloned().zip(internals.iter().cloned()).collect();
    let router = Arc::new(ConsistentHasher::new(publics.clone(), VNODES));

    let mut nodes = Vec::new();
    for (public, public_listener, _internal, internal_listener, metrics_addr, metrics_listener) in
        bound.into_iter()
    {
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
        let heartbeat = Arc::new(Heartbeat::new(
            &public,
            &publics,
            internal_map.clone(),
            HeartbeatConfig {
                interval_secs: 1,
                timeout_ms: 200,
                miss_threshold: 3,
            },
        ));
        {
            let heartbeat = heartbeat.clone();
            tokio::spawn(async move { heartbeat.run().await });
        }
        let ctx = Arc::new(NodeContext {
            store: store.clone(),
            metrics: metrics.clone(),
            aof: None,
            router: router.clone(),
            self_address: public.clone(),
            replicator,
            heartbeat: Some(heartbeat),
            replication_factor: rf,
        });

        let mut handles = Vec::new();
        {
            let ctx = ctx.clone();
            handles.push(tokio::spawn(async move {
                let _ = server::run(public_listener, ctx).await;
            }));
        }
        {
            let ctx = ctx.clone();
            handles.push(tokio::spawn(async move {
                let _ = run_internal(internal_listener, ctx).await;
            }));
        }
        {
            let ctx = ctx.clone();
            handles.push(tokio::spawn(async move {
                let _ = metrics_http::serve(metrics_listener, ctx).await;
            }));
        }

        nodes.push(Node {
            public,
            metrics_addr,
        });
    }
    nodes
}

async fn client(nodes: &[Node]) -> CachexClient<ConsistentHasher> {
    CachexClient::consistent(nodes.iter().map(|n| n.public.clone()).collect(), VNODES)
}

async fn http_get(addr: &str, path: &str) -> (u16, String) {
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();
    let text = String::from_utf8_lossy(&response).to_string();
    let status = text
        .split_once(' ')
        .and_then(|(_, rest)| rest.split(' ').next())
        .and_then(|code| code.parse::<u16>().ok())
        .unwrap_or(0);
    let body = text.split_once("\r\n\r\n").map(|(_, body)| body).unwrap_or("").to_string();
    (status, body)
}

fn keys_primary_on(router: &impl Router, node_addr: &str, count: usize) -> Vec<String> {
    let mut out = Vec::new();
    for i in 0..100_000 {
        let key = format!("m5api:{i}");
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
async fn standalone_health_and_metrics_endpoints() {
    let (public, metrics_addr, metrics_arc) = start_standalone().await;

    let mut connection = Connection::connect(&public).await.unwrap();
    connection
        .command(&Command::Set { key: "user:1".into(), value: b"jill".to_vec(), ttl: None })
        .await
        .unwrap();
    connection
        .command(&Command::Get { key: "user:1".into() })
        .await
        .unwrap();
    match connection.command(&Command::Get { key: "missing".into() }).await.unwrap() {
        Response::NotFound => {}
        other => panic!("expected NotFound, got {other:?}"),
    }

    
    metrics_arc.sample_rates();

    let (status, body) = http_get(&metrics_addr, "/health").await;
    assert_eq!(status, 200, "health status; body={body}");
    let health: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(health["status"], "ok");
    assert_eq!(health["node"], public);
    assert!(health["keys"].as_u64().unwrap() >= 1, "health keys");
    assert!(health["used_bytes"].as_u64().unwrap() > 0);

    let (status, body) = http_get(&metrics_addr, "/metrics").await;
    assert_eq!(status, 200, "metrics status; body={body}");
    let snap: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(snap["node"], public);
    assert_eq!(snap["requests"]["total"].as_u64().unwrap(), 3);
    assert_eq!(snap["requests"]["gets"].as_u64().unwrap(), 2);
    assert_eq!(snap["requests"]["sets"].as_u64().unwrap(), 1);
    assert_eq!(snap["requests"]["hits"].as_u64().unwrap(), 1);
    assert_eq!(snap["requests"]["misses"].as_u64().unwrap(), 1);
    assert!(snap["requests"]["hit_rate"].as_f64().unwrap() > 0.0);
    assert_eq!(snap["rates"]["total"].as_u64().unwrap(), 3);
    assert!(snap["latency"]["get"]["count"].as_u64().unwrap() >= 1);
    assert!(snap["latency"]["set"]["count"].as_u64().unwrap() >= 1);
    assert!(snap["latency"]["get"]["max_us"].as_u64().unwrap() > 0);
    assert_eq!(snap["storage"]["keys"].as_u64().unwrap(), 1);
    assert!(snap["storage"]["used_bytes"].as_u64().unwrap() > 0);
    assert!(snap["aof"].is_null(), "standalone has no AOF");
    assert_eq!(snap["replication"]["sent"].as_u64().unwrap(), 0);
    assert!(snap["peers"].is_null(), "standalone has no peers");

    let (status, _) = http_get(&metrics_addr, "/does-not-exist").await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn cluster_metrics_report_replication_and_peers() {
    let nodes = start_cluster_with_metrics(2, 2).await;
    let client = client(&nodes).await;

    let key = &keys_primary_on(client.router(), &nodes[0].public, 1)[0];
    client.set(key, b"replicated".to_vec(), None).await.unwrap();

    
    let replica = nodes.iter().map(|n| n.public.clone()).find(|a| *a != nodes[0].public).unwrap();
    for _ in 0..100 {
        let mut connection = Connection::connect(&replica).await.unwrap();
        match connection.command(&Command::Get { key: key.to_string() }).await.unwrap() {
            Response::Value(v) if v == b"replicated" => break,
            _ => tokio::time::sleep(Duration::from_millis(50)).await,
        }
    }

    
    let (_, body) = http_get(&nodes[0].metrics_addr, "/metrics").await;
    let snap: Value = serde_json::from_str(&body).unwrap();
    assert!(
        snap["replication"]["sent"].as_u64().unwrap() >= 1,
        "primary should have sent: {body}"
    );
    let peers = snap["peers"].as_object().expect("peers must be present in cluster mode");
    assert!(peers["alive"].as_u64().unwrap() >= 1, "peer should be alive: {body}");

    
    let (_, body) = http_get(&nodes[1].metrics_addr, "/metrics").await;
    let snap: Value = serde_json::from_str(&body).unwrap();
    assert!(
        snap["replication"]["received"].as_u64().unwrap() >= 1,
        "replica should have received: {body}"
    );
}