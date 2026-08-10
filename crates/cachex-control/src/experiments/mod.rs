

pub mod eviction;
pub mod failure;
pub mod hashing;
pub mod replication;
pub mod scalability;
pub mod ttl;

use std::sync::Arc;
use serde::Deserialize;

use crate::events::Event;
use crate::state::AppState;

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct StubParams {
    pub keys: u64,
    pub value_size: usize,
    pub ttl: u64,
    pub clients: usize,
    pub requests: u64,
    pub get_ratio: f64,
    pub node_id: u16,
}

pub async fn run_stub(state: &Arc<AppState>, name: &str) -> serde_json::Value {
    state.emit(Event::ExperimentPhase {
        name: name.to_string(),
        detail: "starting".to_string(),
    });
    let result = serde_json::json!({
        "status": "not_implemented",
        "message": format!("experiment `{name}` is not implemented in this build"),
    });
    state.emit(Event::ExperimentDone {
        name: name.to_string(),
        result: result.clone(),
    });
    *state.last_experiment.lock().unwrap() = Some(result.clone());
    result
}

pub fn finish_experiment(state: &Arc<AppState>, name: &str, result: serde_json::Value) -> serde_json::Value {
    state.emit(Event::ExperimentDone {
        name: name.to_string(),
        result: result.clone(),
    });
    *state.last_experiment.lock().unwrap() = Some(result.clone());
    result
}

pub async fn ensure_cluster(state: &Arc<AppState>) -> Result<(), String> {
    let spec = state.spec.lock().unwrap().clone();
    let mut all_up = true;
    for node_id in 1..=spec.node_count {
        if !state.node_alive(node_id).await {
            all_up = false;
            break;
        }
    }
    if all_up {
        return Ok(());
    }
    crate::nodes::start_cluster(state)
        .await
        .map_err(|e| format!("failed to start cluster: {e}"))
}

#[derive(Debug, Clone, Default)]
pub struct ClusterMetrics {
    pub used_bytes: u64,
    pub keys: u64,
    pub evictions: u64,
    pub repl_sent: u64,
    pub repl_received: u64,
}

pub async fn cluster_metrics(state: &Arc<AppState>) -> ClusterMetrics {
    let spec = state.spec.lock().unwrap().clone();
    let mut m = ClusterMetrics::default();
    for node_id in 1..=spec.node_count {
        if let Ok(s) = crate::metrics::metrics_of(state, node_id).await {
            m.used_bytes += s.storage.used_bytes;
            m.keys += s.storage.keys;
            m.evictions += s.storage.evictions;
            if let Some(replication) = s.replication {
                m.repl_sent += replication.sent;
                m.repl_received += replication.received;
            }
        }
    }
    m
}