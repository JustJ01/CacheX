

use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub async fn get_json<T: serde::de::DeserializeOwned>(
    host: &str,
    port: u16,
    path: &str,
    timeout_ms: u64,
) -> Result<T, String> {
    let addr = format!("{host}:{port}");
    let stream = tokio::time::timeout(
        Duration::from_millis(timeout_ms),
        TcpStream::connect(&addr),
    )
    .await
    .map_err(|_| format!("connect to {addr} timed out"))?
    .map_err(|e| format!("connect to {addr}: {e}"))?;

    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\n\r\n"
    );
    let mut stream = stream;
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|e| format!("write to {addr}: {e}"))?;

    let mut buf = Vec::with_capacity(4096);
    tokio::time::timeout(
        Duration::from_millis(timeout_ms),
        stream.read_to_end(&mut buf),
    )
    .await
    .map_err(|_| format!("read from {addr} timed out"))?
    .map_err(|e| format!("read from {addr}: {e}"))?;

    let text = String::from_utf8_lossy(&buf);
    let body = text
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .unwrap_or(&text);
    serde_json::from_str(body).map_err(|e| format!("bad json from {addr}: {e}"))
}