

use cachex_core::config::AofConfig;
use cachex_server::{aof, metrics::Metrics, node::NodeContext, server, storage::CacheStore};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

fn temp_path(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("cachex-aof-integration-{tag}-{}.aof", std::process::id()));
    path
}

fn aof_config(path: &PathBuf) -> AofConfig {
    AofConfig {
        enabled: true,
        path: path.to_string_lossy().into_owned(),
        fsync: "always".to_string(),
        fsync_interval_secs: 1,
        rewrite_threshold_bytes: 0,
    }
}

async fn start(store: Arc<CacheStore>, aof_cfg: AofConfig) -> (SocketAddr, Arc<Metrics>) {
    let metrics = Arc::new(Metrics::new());
    let aof = aof::Aof::new(&aof_cfg).await.unwrap();
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let addr = listener.local_addr().unwrap();
    let ctx = NodeContext::standalone(store, metrics.clone(), aof, addr.to_string());
    tokio::spawn(async move {
        let _ = server::run(listener, ctx).await;
    });
    (addr, metrics)
}

async fn ask(addr: SocketAddr, line: &str) -> String {
    let stream = TcpStream::connect(addr).await.unwrap();
    let (reader, mut writer) = stream.into_split();
    writer.write_all(line.as_bytes()).await.unwrap();
    let mut reader = BufReader::new(reader);
    let mut response = String::new();
    reader.read_line(&mut response).await.unwrap();
    response.trim_end().to_string()
}

#[tokio::test]
async fn server_writes_aof_and_replay_restores_state() {
    let path = temp_path("server");
    let _ = std::fs::remove_file(&path);

    let store = Arc::new(CacheStore::new(1_000_000));
    let (addr, _metrics) = start(store.clone(), aof_config(&path)).await;

    assert_eq!(ask(addr, "SET user:101 Jill\n").await, "OK");
    assert_eq!(ask(addr, "GET user:101\n").await, "VALUE Jill");
    assert_eq!(ask(addr, "SET otp 1234 60\n").await, "OK");
    assert_eq!(ask(addr, "DELETE user:101\n").await, "OK");

    
    let fresh = CacheStore::new(1_000_000);
    let report = aof::replay(&path, &fresh).unwrap();
    assert_eq!(report.applied, 3);
    assert_eq!(fresh.get("user:101").0, None, "deleted key must not return");
    assert_eq!(fresh.get("otp").0, Some(b"1234".to_vec()));

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn info_reports_aof_enabled() {
    let path = temp_path("info");
    let _ = std::fs::remove_file(&path);

    let store = Arc::new(CacheStore::new(1_000_000));
    let (addr, _metrics) = start(store.clone(), aof_config(&path)).await;

    let _ = ask(addr, "SET k v\n").await;
    let response = ask(addr, "INFO\n").await;
    assert!(response.contains("aof_writes="), "INFO should report AOF writes: {response}");
    assert!(response.contains("recovery_ms="), "INFO should report recovery time: {response}");

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn disabled_aof_reports_off() {
    let store = Arc::new(CacheStore::new(1_000_000));
    let metrics = Arc::new(Metrics::new());
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let addr = listener.local_addr().unwrap();
    let ctx = NodeContext::standalone(store, metrics, None, addr.to_string());
    tokio::spawn(async move {
        let _ = server::run(listener, ctx).await;
    });

    let _ = ask(addr, "SET k v\n").await;
    let response = ask(addr, "INFO\n").await;
    assert!(response.contains("aof=off"), "INFO should report AOF off: {response}");
    assert!(response.contains("peers=off"), "INFO should report peers off: {response}");
}