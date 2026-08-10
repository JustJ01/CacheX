

use crate::aof::Aof;
use crate::heartbeat::Heartbeat;
use crate::metrics::Metrics;
use crate::replication::Replicator;
use crate::storage::CacheStore;
use cachex_core::hashing::{ConsistentHasher, Router};
use std::sync::Arc;

pub struct NodeContext {
    pub store: Arc<CacheStore>,
    pub metrics: Arc<Metrics>,
    pub aof: Option<Arc<Aof>>,
    pub router: Arc<ConsistentHasher>,
    pub self_address: String,
    pub replicator: Option<Arc<Replicator>>,
    pub heartbeat: Option<Arc<Heartbeat>>,
    pub replication_factor: u32,
}

impl NodeContext {
    
    
    pub fn standalone(
        store: Arc<CacheStore>,
        metrics: Arc<Metrics>,
        aof: Option<Arc<Aof>>,
        self_address: String,
    ) -> Arc<Self> {
        let router = Arc::new(ConsistentHasher::new(vec![self_address.clone()], 1));
        Arc::new(NodeContext {
            store,
            metrics,
            aof,
            router,
            self_address,
            replicator: None,
            heartbeat: None,
            replication_factor: 1,
        })
    }

    
    pub fn is_primary(&self, key: &str) -> bool {
        self.router.primary(key) == self.self_address.as_str()
    }
}