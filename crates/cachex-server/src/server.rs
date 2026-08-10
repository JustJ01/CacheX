

use crate::metrics::CommandKind;
use crate::node::NodeContext;
use cachex_core::protocol::{encode_response, parse_command, Command, Response};
use std::io;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

const MAX_LINE_BYTES: usize = 64 * 1024;

pub async fn run(listener: TcpListener, ctx: Arc<NodeContext>) -> io::Result<()> {
    loop {
        let (socket, peer) = listener.accept().await?;
        let ctx = ctx.clone();
        tokio::spawn(async move {
            if let Err(error) = handle_connection(socket, &ctx).await {
                eprintln!("connection {peer} error: {error}");
            }
        });
    }
}

async fn handle_connection(stream: TcpStream, ctx: &Arc<NodeContext>) -> io::Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut buf = Vec::with_capacity(256);

    loop {
        buf.clear();
        let read = reader.read_until(b'\n', &mut buf).await?;
        if read == 0 {
            return Ok(());
        }
        if buf.len() > MAX_LINE_BYTES {
            writer
                .write_all(&encode_response(&Response::Error(
                    "request too large".to_string(),
                )))
                .await?;
            continue;
        }

        let command = match parse_command(&String::from_utf8_lossy(&buf)) {
            Ok(command) => command,
            Err(error) => {
                writer
                    .write_all(&encode_response(&Response::Error(error.to_string())))
                    .await?;
                continue;
            }
        };

        let response = handle_command(command, ctx).await;
        writer.write_all(&encode_response(&response)).await?;
        writer.flush().await?;
    }
}

async fn handle_command(command: Command, ctx: &Arc<NodeContext>) -> Response {
    ctx.metrics.record_request();
    let kind = CommandKind::of(&command);
    let start = Instant::now();
    let response = dispatch(command, ctx).await;
    ctx.metrics.record_latency(kind, start.elapsed());
    response
}

async fn dispatch(command: Command, ctx: &Arc<NodeContext>) -> Response {
    match command {
        Command::Ping => Response::Pong,
        Command::Info => ctx.metrics.info(&ctx.store, ctx.aof.as_deref(), ctx.heartbeat.as_deref()),
        Command::Set { key, value, ttl } => {
            ctx.metrics.record_set();
            if let Some(aof) = ctx.aof.as_ref() {
                let cmd = Command::Set {
                    key: key.clone(),
                    value: value.clone(),
                    ttl,
                };
                if let Err(error) = aof.append(&cmd).await {
                    return Response::Error(format!("aof append failed: {error}"));
                }
            }
            ctx.store.set(&key, value.clone(), ttl);
            if let Some(replicator) = ctx.replicator.as_ref() {
                if ctx.is_primary(&key) {
                    let cmd = Command::Set {
                        key: key.clone(),
                        value: value.clone(),
                        ttl,
                    };
                    let (sent, failed) = replicator.replicate(&key, &cmd).await;
                    ctx.metrics.record_replication(sent, failed);
                }
            }
            Response::Ok
        }
        Command::Get { key } => {
            ctx.metrics.record_get();
            let (value, _expired) = ctx.store.get(&key);
            match value {
                Some(value) => {
                    ctx.metrics.record_hit();
                    Response::Value(value)
                }
                None => {
                    ctx.metrics.record_miss();
                    Response::NotFound
                }
            }
        }
        Command::Delete { key } => {
            ctx.metrics.record_delete();
            if let Some(aof) = ctx.aof.as_ref() {
                let cmd = Command::Delete { key: key.clone() };
                if let Err(error) = aof.append(&cmd).await {
                    return Response::Error(format!("aof append failed: {error}"));
                }
            }
            if ctx.store.delete(&key) {
                if let Some(replicator) = ctx.replicator.as_ref() {
                    if ctx.is_primary(&key) {
                        let cmd = Command::Delete { key: key.clone() };
                        let (sent, failed) = replicator.replicate(&key, &cmd).await;
                        ctx.metrics.record_replication(sent, failed);
                    }
                }
                Response::Ok
            } else {
                Response::NotFound
            }
        }
    }
}