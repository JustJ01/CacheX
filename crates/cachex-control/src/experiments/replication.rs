

use std::sync::Arc;
use std::time::Instant;

use cachex_bench::report::Report;
use serde::{Deserialize, Serialize};

use crate::events::Event;
use crate::experiments::{cluster_metrics, ensure_cluster, finish_experiment};
use crate::load::LoadParams;
use crate::nodes;
use crate::state::AppState;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ReplicationParams {
    pub clients: usize,
    pub requests: u64,
    pub get_ratio: f64,
    pub keys: u64,
    pub value_size: usize,
    pub key_order: String,
    pub ttl: u64,
    pub seed: u64,
    pub vnodes: usize,
    pub router: String,
    
    pub factors: Vec<u32>,
}

impl Default for ReplicationParams {
    fn default() -> Self {
        
        
        ReplicationParams {
            clients: 16,
            requests: 50_000,
            get_ratio: 0.5,
            keys: 20_000,
            value_size: 128,
            key_order: "uniform".to_string(),
            ttl: 0,
            seed: 42,
            vnodes: 100,
            router: "consistent".to_string(),
            factors: vec![1, 2],
        }
    }
}

impl ReplicationParams {
    fn workload(&self) -> LoadParams {
        LoadParams {
            clients: self.clients,
            requests: self.requests,
            get_ratio: self.get_ratio,
            keys: self.keys,
            value_size: self.value_size,
            key_order: self.key_order.clone(),
            ttl: self.ttl,
            seed: self.seed,
            vnodes: self.vnodes,
            router: self.router.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct ReplicationRow {
    rf: u32,
    copies: u64,
    report: Report,
    used_bytes: u64,
    keys: u64,
    repl_sent: u64,
    repl_received: u64,
}

fn row_json(row: &ReplicationRow) -> serde_json::Value {
    serde_json::json!({
        "rf": row.rf,
        "copies": row.copies,
        "report": row.report,
        "used_bytes": row.used_bytes,
        "keys": row.keys,
        "repl_sent": row.repl_sent,
        "repl_received": row.repl_received,
    })
}

pub async fn run_replication(state: &Arc<AppState>, params: ReplicationParams) -> serde_json::Value {
    let started = Instant::now();
    let factors: Vec<u32> = if params.factors.is_empty() {
        vec![1, 2]
    } else {
        params.factors.iter().copied().map(|f| f.clamp(1, 4)).collect()
    };

    state.emit(Event::ExperimentPhase {
        name: "replication".to_string(),
        detail: "ensuring cluster is running".to_string(),
    });
    if let Err(message) = ensure_cluster(state).await {
        return finish_experiment(state, "replication", serde_json::json!({
            "status": "error",
            "message": message,
        }));
    }

    let mut rows = Vec::new();
    for &factor in &factors {
        state.emit(Event::ExperimentPhase {
            name: "replication".to_string(),
            detail: format!("setting replication factor {factor}"),
        });
        {
            let mut spec = state.spec.lock().unwrap();
            spec.replication_factor = factor;
        }
        
        
        
        let _ = nodes::stop_cluster(state).await;
        if let Err(e) = nodes::clear_aofs(state) {
            return finish_experiment(state, "replication", serde_json::json!({
                "status": "error",
                "message": format!("failed to clear AOFs: {e}"),
            }));
        }
        if let Err(e) = nodes::start_cluster(state).await {
            return finish_experiment(state, "replication", serde_json::json!({
                "status": "error",
                "message": format!("failed to start cluster at RF={factor}: {e}"),
            }));
        }
        
        tokio::time::sleep(std::time::Duration::from_millis(1200)).await;

        state.emit(Event::ExperimentPhase {
            name: "replication".to_string(),
            detail: format!(
                "running workload at RF={factor} ({} clients, {} requests)",
                params.clients, params.requests
            ),
        });
        let report = match crate::load::run_bench(state, &params.workload()).await {
            Ok(report) => report,
            Err(message) => {
                return finish_experiment(state, "replication", serde_json::json!({
                    "status": "error",
                    "message": format!("workload at RF={factor} failed: {message}"),
                }))
            }
        };
        let metrics = cluster_metrics(state).await;
        rows.push(ReplicationRow {
            rf: factor,
            copies: factor as u64,
            report,
            used_bytes: metrics.used_bytes,
            keys: metrics.keys,
            repl_sent: metrics.repl_sent,
            repl_received: metrics.repl_received,
        });
    }

    let mut result = serde_json::json!({
        "status": "ok",
        "factors": factors,
        "rows": rows.iter().map(row_json).collect::<Vec<_>>(),
        "elapsed_ms": started.elapsed().as_millis(),
        "finding": "Replication improves redundancy at the cost of additional memory and write throughput.",
    });

    if rows.len() >= 2 {
        let base = &rows[0];
        let other = &rows[1];
        let throughput_pct = if base.report.ops_per_sec > 0.0 {
            ((other.report.ops_per_sec - base.report.ops_per_sec) / base.report.ops_per_sec) * 100.0
        } else {
            0.0
        };
        let memory_pct = if base.used_bytes > 0 {
            ((other.used_bytes as f64 - base.used_bytes as f64) / base.used_bytes as f64) * 100.0
        } else {
            0.0
        };
        result["comparison"] = serde_json::json!({
            "throughput_rf1": base.report.ops_per_sec,
            "throughput_rf2": other.report.ops_per_sec,
            "throughput_change_pct": (throughput_pct * 10.0).round() / 10.0,
            "memory_rf1": base.used_bytes,
            "memory_rf2": other.used_bytes,
            "memory_change_pct": (memory_pct * 10.0).round() / 10.0,
            "replication_sent": other.repl_sent,
        });
    }

    finish_experiment(state, "replication", result)
}