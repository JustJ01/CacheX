

use crate::node::NodeContext;
use cachex_core::protocol::Command;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};

pub use cachex_core::internal::{InternalMessage, InternalResponse};

pub const INTERNAL_PORT_OFFSET: u16 = 1000;

const MAX_INTERNAL_MSG_BYTES: usize = 16 * 1024 * 1024;

pub fn internal_address(public: &str) -> String {
    match public.rsplit_once(':') {
        Some((host, port)) => match port.parse::<u16>() {
            Ok(p) => format!("{host}:{}", p + INTERNAL_PORT_OFFSET),
            Err(_) => public.to_string(),
        },
        None => public.to_string(),
    }
}

pub fn internal_map_for_cluster(nodes: &[String]) -> HashMap<String, String> {
    nodes
        .iter()
        .map(|node| (node.clone(), internal_address(node)))
        .collect()
}

fn io_error(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

pub async fn write_message<W, T>(writer: &mut W, msg: &T) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
    T: Serialize,
{
    let bytes = bincode::serialize(msg).map_err(|e| io_error(e.to_string()))?;
    writer.write_all(&(bytes.len() as u32).to_le_bytes()).await?;
    writer.write_all(&bytes).await?;
    Ok(())
}

pub async fn read_message<R, T>(reader: &mut R) -> io::Result<T>
where
    R: AsyncRead + Unpin,
    T: DeserializeOwned,
{
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).await?;
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > MAX_INTERNAL_MSG_BYTES {
        return Err(io_error(format!("internal message too large: {len} bytes")));
    }
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).await?;
    bincode::deserialize(&buf).map_err(|e| io_error(e.to_string()))
}

pub struct InternalConnection {
    reader: BufReader<OwnedReadHalf>,
    writer: OwnedWriteHalf,
}

impl InternalConnection {
    pub async fn connect(address: &str) -> io::Result<Self> {
        let stream = TcpStream::connect(address).await?;
        let (reader, writer) = stream.into_split();
        Ok(InternalConnection {
            reader: BufReader::new(reader),
            writer,
        })
    }

    pub async fn request(&mut self, msg: &InternalMessage) -> io::Result<InternalResponse> {
        write_message(&mut self.writer, msg).await?;
        read_message(&mut self.reader).await
    }
}

pub async fn run_internal(listener: TcpListener, ctx: Arc<NodeContext>) -> io::Result<()> {
    loop {
        let (socket, peer) = listener.accept().await?;
        let ctx = ctx.clone();
        tokio::spawn(async move {
            if let Err(error) = handle_internal_connection(socket, &ctx).await {
                eprintln!("internal connection {peer} error: {error}");
            }
        });
    }
}

async fn handle_internal_connection(
    stream: TcpStream,
    ctx: &NodeContext,
) -> io::Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    loop {
        let msg: InternalMessage = match read_message(&mut reader).await {
            Ok(msg) => msg,
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(e) => {
                let _ =
                    write_message(&mut writer, &InternalResponse::Error(e.to_string())).await;
                return Ok(());
            }
        };
        let response = handle_internal_message(msg, ctx).await;
        write_message(&mut writer, &response).await?;
    }
}

async fn handle_internal_message(msg: InternalMessage, ctx: &NodeContext) -> InternalResponse {
    match msg {
        InternalMessage::Ping => InternalResponse::Pong,
        InternalMessage::Upsert { key, value, ttl } => {
            ctx.store.set(&key, value.clone(), ttl);
            if let Some(aof) = ctx.aof.as_ref() {
                let cmd = Command::Set {
                    key: key.clone(),
                    value: value.clone(),
                    ttl,
                };
                if let Err(error) = aof.append(&cmd).await {
                    ctx.metrics.record_replication_failed();
                    return InternalResponse::Error(error.to_string());
                }
            }
            ctx.metrics.record_replication_received();
            InternalResponse::Ok
        }
        InternalMessage::Delete { key } => {
            ctx.store.delete(&key);
            if let Some(aof) = ctx.aof.as_ref() {
                let cmd = Command::Delete { key: key.clone() };
                if let Err(error) = aof.append(&cmd).await {
                    ctx.metrics.record_replication_failed();
                    return InternalResponse::Error(error.to_string());
                }
            }
            ctx.metrics.record_replication_received();
            InternalResponse::Ok
        }
    }
}