

use crate::pool::ConnectionPool;
use cachex_core::hashing::{ConsistentHasher, ModuloHasher, Router};
use cachex_core::protocol::{Command, Response};
use std::fmt;
use std::io;
use std::sync::Arc;

#[derive(Debug)]
pub enum ClientError {
    Io(io::Error),
    Remote(String),
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClientError::Io(error) => write!(f, "io error: {error}"),
            ClientError::Remote(message) => write!(f, "remote error: {message}"),
        }
    }
}

impl std::error::Error for ClientError {}

impl From<io::Error> for ClientError {
    fn from(error: io::Error) -> Self {
        ClientError::Io(error)
    }
}

pub struct CachexClient<R: Router> {
    router: R,
    pool: Arc<ConnectionPool>,
}

impl<R: Router> CachexClient<R> {
    pub fn new(router: R) -> Self {
        CachexClient {
            router,
            pool: Arc::new(ConnectionPool::new()),
        }
    }

    pub fn router(&self) -> &R {
        &self.router
    }

    pub async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, ClientError> {
        let node = self.router.primary(key).to_string();
        let command = Command::Get { key: key.to_string() };
        match self.pool.command(&node, &command).await? {
            Response::Value(value) => Ok(Some(value)),
            Response::NotFound => Ok(None),
            Response::Error(message) => Err(ClientError::Remote(message)),
            other => Err(ClientError::Remote(format!("unexpected response {other:?}"))),
        }
    }

    pub async fn set(&self, key: &str, value: Vec<u8>, ttl: Option<u64>) -> Result<(), ClientError> {
        let node = self.router.primary(key).to_string();
        let command = Command::Set {
            key: key.to_string(),
            value,
            ttl,
        };
        match self.pool.command(&node, &command).await? {
            Response::Ok => Ok(()),
            Response::Error(message) => Err(ClientError::Remote(message)),
            other => Err(ClientError::Remote(format!("unexpected response {other:?}"))),
        }
    }

    pub async fn delete(&self, key: &str) -> Result<bool, ClientError> {
        let node = self.router.primary(key).to_string();
        let command = Command::Delete { key: key.to_string() };
        match self.pool.command(&node, &command).await? {
            Response::Ok => Ok(true),
            Response::NotFound => Ok(false),
            Response::Error(message) => Err(ClientError::Remote(message)),
            other => Err(ClientError::Remote(format!("unexpected response {other:?}"))),
        }
    }
}

impl CachexClient<ConsistentHasher> {
    
    pub fn consistent(nodes: Vec<String>, vnodes: usize) -> Self {
        CachexClient::new(ConsistentHasher::new(nodes, vnodes))
    }
}

impl CachexClient<ModuloHasher> {
    
    pub fn modulo(nodes: Vec<String>) -> Self {
        CachexClient::new(ModuloHasher::new(nodes))
    }
}