

use cachex_server::{metrics::Metrics, node::NodeContext, server, storage::CacheStore};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

async fn start() -> SocketAddr {
    let store = Arc::new(CacheStore::new(1_000_000));
    let metrics = Arc::new(Metrics::new());
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let addr = listener.local_addr().unwrap();
    let ctx = NodeContext::standalone(store, metrics, None, addr.to_string());
    tokio::spawn(async move {
        let _ = server::run(listener, ctx).await;
    });
    addr
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
async fn set_get_delete_round_trip() {
    let addr = start().await;
    assert_eq!(ask(addr, "PING\n").await, "PONG");
    assert_eq!(ask(addr, "SET user:101 Jill\n").await, "OK");
    assert_eq!(ask(addr, "GET user:101\n").await, "VALUE Jill");
    assert_eq!(ask(addr, "GET missing\n").await, "NOTFOUND");
    assert_eq!(ask(addr, "DELETE user:101\n").await, "OK");
    assert_eq!(ask(addr, "GET user:101\n").await, "NOTFOUND");
    assert_eq!(ask(addr, "DELETE user:101\n").await, "NOTFOUND");
}

#[tokio::test]
async fn set_with_value_spaces() {
    let addr = start().await;
    assert_eq!(ask(addr, "SET note hello world\n").await, "OK");
    assert_eq!(ask(addr, "GET note\n").await, "VALUE hello world");
}

#[tokio::test]
async fn ttl_expires() {
    let addr = start().await;
    assert_eq!(ask(addr, "SET otp 1234 1\n").await, "OK");
    assert_eq!(ask(addr, "GET otp\n").await, "VALUE 1234");
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    assert_eq!(ask(addr, "GET otp\n").await, "NOTFOUND");
}

#[tokio::test]
async fn unknown_command_returns_error() {
    let addr = start().await;
    assert!(ask(addr, "BOGUS x\n").await.starts_with("ERROR"));
    assert!(ask(addr, "GET a b\n").await.starts_with("ERROR"));
}

#[tokio::test]
async fn info_returns_stats() {
    let addr = start().await;
    let _ = ask(addr, "SET k v\n").await;
    let response = ask(addr, "INFO\n").await;
    assert!(response.starts_with("INFO "));
    assert!(response.contains("keys=1"), "INFO should report one key: {response}");
    assert!(response.contains("hits="), "INFO should report hits: {response}");
}

#[tokio::test]
async fn concurrent_clients() {
    let addr = start().await;
    let mut handles = Vec::new();
    for i in 0..50 {
        handles.push(tokio::spawn(async move {
            let key = format!("k{i}");
            let value = format!("v{i}");
            assert_eq!(ask(addr, &format!("SET {key} {value}\n")).await, "OK");
            assert_eq!(ask(addr, &format!("GET {key}\n")).await, format!("VALUE {value}"));
        }));
    }
    for handle in handles {
        handle.await.unwrap();
    }
}

#[tokio::test]
async fn pipelined_requests_on_one_connection() {
    let addr = start().await;
    let stream = TcpStream::connect(addr).await.unwrap();
    let (reader, mut writer) = stream.into_split();
    writer
        .write_all(b"SET a 1\nGET a\nSET b 2\nGET b\n")
        .await
        .unwrap();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    let mut responses = Vec::new();
    for _ in 0..4 {
        line.clear();
        reader.read_line(&mut line).await.unwrap();
        responses.push(line.trim_end().to_string());
    }
    assert_eq!(responses, vec!["OK", "VALUE 1", "OK", "VALUE 2"]);
}