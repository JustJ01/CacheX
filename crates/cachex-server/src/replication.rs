

use crate::internal::InternalConnection;
use cachex_core::hashing::{ConsistentHasher, Router};
use cachex_core::internal::{InternalMessage, InternalResponse};
use cachex_core::protocol::Command;
use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

pub struct Replicator {
    router: Arc<ConsistentHasher>,
    internal: HashMap<String, String>,
    connections: Mutex<HashMap<String, InternalConnection>>,
    factor: u32,
    
    
    send_timeout: Duration,
}

impl Replicator {
    pub fn new(
        router: Arc<ConsistentHasher>,
        internal: HashMap<String, String>,
        factor: u32,
    ) -> Self {
        Replicator {
            router,
            internal,
            connections: Mutex::new(HashMap::new()),
            factor,
            send_timeout: Duration::from_secs(2),
        }
    }

    
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.send_timeout = timeout;
        self
    }

    
    
    
    
    
    pub async fn send(&self, internal_addr: &str, msg: &InternalMessage) -> io::Result<()> {
        let mut connections = self.connections.lock().await;
        if !connections.contains_key(internal_addr) {
            let connect = tokio::time::timeout(
                self.send_timeout,
                InternalConnection::connect(internal_addr),
            )
            .await;
            let conn = match connect {
                Ok(Ok(conn)) => conn,
                Ok(Err(e)) => return Err(e),
                Err(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "replica connect timed out",
                    ))
                }
            };
            connections.insert(internal_addr.to_string(), conn);
        }

        let request = connections
            .get_mut(internal_addr)
            .expect("connection just inserted")
            .request(msg);
        let result = tokio::time::timeout(self.send_timeout, request).await;
        match result {
            Ok(Ok(InternalResponse::Ok)) | Ok(Ok(InternalResponse::Pong)) => Ok(()),
            Ok(Ok(InternalResponse::Error(message))) => Err(io::Error::new(
                io::ErrorKind::Other,
                format!("replica error: {message}"),
            )),
            Ok(Err(e)) => {
                connections.remove(internal_addr);
                Err(e)
            }
            Err(_) => {
                connections.remove(internal_addr);
                Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "replica request timed out",
                ))
            }
        }
    }

    
    
    pub async fn replicate(&self, key: &str, command: &Command) -> (u64, u64) {
        let replica_count = self.factor.saturating_sub(1) as usize;
        let replicas = self.router.replicas(key, replica_count);
        let mut delivered = 0u64;
        let mut failed = 0u64;
        for public in replicas {
            let Some(internal) = self.internal.get(public) else {
                failed += 1;
                continue;
            };
            let msg = match command {
                Command::Set { key, value, ttl } => InternalMessage::Upsert {
                    key: key.clone(),
                    value: value.clone(),
                    ttl: *ttl,
                },
                Command::Delete { key } => InternalMessage::Delete { key: key.clone() },
                _ => continue,
            };
            match self.send(internal, &msg).await {
                Ok(()) => delivered += 1,
                Err(_) => failed += 1,
            }
        }
        (delivered, failed)
    }
}