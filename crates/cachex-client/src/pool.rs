

use crate::connection::Connection;
use cachex_core::protocol::{Command, Response};
use std::collections::HashMap;
use std::io;
use std::sync::{Arc, Mutex};
use tokio::sync::Mutex as AsyncMutex;

struct Slot {
    connection: AsyncMutex<Option<Connection>>,
}

pub struct ConnectionPool {
    slots: Mutex<HashMap<String, Arc<Slot>>>,
}

impl Default for ConnectionPool {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnectionPool {
    pub fn new() -> Self {
        ConnectionPool {
            slots: Mutex::new(HashMap::new()),
        }
    }

    
    
    pub async fn command(&self, address: &str, command: &Command) -> io::Result<Response> {
        let slot = {
            let mut slots = self.slots.lock().expect("connection pool lock poisoned");
            slots
                .entry(address.to_string())
                .or_insert_with(|| Arc::new(Slot {
                    connection: AsyncMutex::new(None),
                }))
                .clone()
        };

        let mut guard = slot.connection.lock().await;
        if guard.is_none() {
            *guard = Some(Connection::connect(address).await?);
        }
        let connection = guard.as_mut().expect("connection just established");
        match connection.command(command).await {
            Ok(response) => Ok(response),
            Err(error) => {
                *guard = None;
                Err(error)
            }
        }
    }

    
    pub fn tracked(&self) -> usize {
        self.slots.lock().expect("connection pool lock poisoned").len()
    }
}