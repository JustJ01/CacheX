

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PeerCounts {
    pub alive: usize,
    pub suspected: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct StorageInfo {
    pub keys: u64,
    pub used_bytes: u64,
    pub max_bytes: u64,
    pub evictions: u64,
    pub ttl_expirations: u64,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RatesInfo {
    pub total: u64,
    pub get: u64,
    pub set: u64,
    pub delete: u64,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RequestsInfo {
    pub total: u64,
    pub gets: u64,
    pub sets: u64,
    pub deletes: u64,
    pub hits: u64,
    pub misses: u64,
    pub hit_rate: f64,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ReplicationInfo {
    pub sent: u64,
    pub failed: u64,
    pub received: u64,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct AofInfo {
    pub bytes_written: u64,
    pub write_count: u64,
    pub fsync_count: u64,
    pub rewrite_count: u64,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct NodeSnapshot {
    pub node: String,
    pub uptime_secs: u64,
    pub recovery_ms: u64,
    pub peers: Option<PeerCounts>,
    pub storage: StorageInfo,
    pub rates: RatesInfo,
    pub requests: RequestsInfo,
    pub replication: Option<ReplicationInfo>,
    pub aof: Option<AofInfo>,
}

use crate::state::AppState;
use std::sync::Arc;

pub async fn metrics_of(state: &Arc<AppState>, node_id: u16) -> Result<NodeSnapshot, String> {
    let spec = state.spec.lock().unwrap().clone();
    crate::http::get_json::<NodeSnapshot>(
        &spec.host,
        spec.metrics_port(node_id),
        "/metrics",
        1500,
    )
    .await
}

pub async fn cluster_peer_counts(state: &Arc<AppState>) -> (usize, usize, usize) {
    let spec = state.spec.lock().unwrap().clone();
    let mut alive = 0;
    let mut suspected = 0;
    let mut failed = 0;
    for node_id in 1..=spec.node_count {
        match metrics_of(state, node_id).await {
            Ok(snapshot) => {
                if let Some(peers) = snapshot.peers {
                    alive += peers.alive;
                    suspected += peers.suspected;
                    failed += peers.failed;
                }
            }
            Err(_) => {
                failed += 1;
            }
        }
    }
    (alive, suspected, failed)
}