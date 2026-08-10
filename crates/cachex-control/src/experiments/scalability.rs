

use std::sync::Arc;
use std::time::Instant;

use serde::Deserialize;

use crate::events::Event;
use crate::experiments::{ensure_cluster, finish_experiment};
use crate::load::LoadParams;
use crate::state::AppState;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ScalabilityParams {
    
    pub clients: Vec<usize>,
    
    pub requests: u64,
    pub get_ratio: f64,
    pub keys: u64,
    pub value_size: usize,
    pub key_order: String,
    pub ttl: u64,
    pub seed: u64,
    pub vnodes: usize,
    pub router: String,
}

impl Default for ScalabilityParams {
    fn default() -> Self {
        ScalabilityParams {
            clients: vec![10, 50, 100, 250, 500, 1000],
            requests: 30_000,
            get_ratio: 0.9,
            keys: 20_000,
            value_size: 128,
            key_order: "uniform".to_string(),
            ttl: 0,
            seed: 42,
            vnodes: 100,
            router: "consistent".to_string(),
        }
    }
}

pub async fn run_scalability(state: &Arc<AppState>, params: ScalabilityParams) -> serde_json::Value {
    let started = Instant::now();
    let clients_list: Vec<usize> = if params.clients.is_empty() {
        vec![10, 50, 100, 250, 500, 1000]
    } else {
        params.clients.iter().copied().map(|c| c.max(1)).collect()
    };
    let requests = params.requests.max(1);

    state.emit(Event::ExperimentPhase {
        name: "scalability".to_string(),
        detail: "ensuring cluster is running".to_string(),
    });
    if let Err(message) = ensure_cluster(state).await {
        return finish_experiment(state, "scalability", serde_json::json!({
            "status": "error",
            "message": message,
        }));
    }

    let mut points = Vec::new();
    for &clients in &clients_list {
        state.emit(Event::ExperimentPhase {
            name: "scalability".to_string(),
            detail: format!("running {clients} clients × {requests} requests"),
        });
        let run = LoadParams {
            clients,
            requests,
            get_ratio: params.get_ratio,
            keys: params.keys,
            value_size: params.value_size,
            key_order: params.key_order.clone(),
            ttl: params.ttl,
            seed: params.seed,
            vnodes: params.vnodes,
            router: params.router.clone(),
        };
        let report = match crate::load::run_bench(state, &run).await {
            Ok(report) => report,
            Err(message) => {
                return finish_experiment(state, "scalability", serde_json::json!({
                    "status": "error",
                    "message": format!("workload at {clients} clients failed: {message}"),
                }))
            }
        };
        points.push(serde_json::json!({
            "clients": clients,
            "ops_per_sec": report.ops_per_sec,
            "p99_us": report.p99_us,
            "errors": report.errors,
            "total_secs": report.total_secs,
        }));
    }

    let peak = points
        .iter()
        .max_by(|a, b| {
            a["ops_per_sec"]
                .as_f64()
                .unwrap_or(0.0)
                .total_cmp(&b["ops_per_sec"].as_f64().unwrap_or(0.0))
        })
        .cloned()
        .unwrap_or(serde_json::json!({}));

    let rollover = points
        .last()
        .map(|last| {
            peak["ops_per_sec"]
                .as_f64()
                .unwrap_or(0.0)
                > last["ops_per_sec"].as_f64().unwrap_or(0.0)
        })
        .unwrap_or(false);

    finish_experiment(state, "scalability", serde_json::json!({
        "status": "ok",
        "clients": clients_list,
        "points": points,
        "peak": peak,
        "rollover": rollover,
        "finding": "More clients don't necessarily mean more throughput; eventually contention and queueing dominate.",
        "elapsed_ms": started.elapsed().as_millis(),
    }))
}