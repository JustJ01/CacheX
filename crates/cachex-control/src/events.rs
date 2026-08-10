

use cachex_bench::report::Report;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    ClusterStarted { node_count: u16 },
    ClusterStopped,
    NodeStarted { id: u16, address: String },
    NodeKilled { id: u16, address: String },
    NodeSuspected { id: u16, address: String },
    NodeFailed { id: u16, address: String },
    NodeRestarted { id: u16, address: String },
    NodeHealthy { id: u16, address: String },
    AofReplayed { id: u16, ms: u64 },
    LoadStarted {
        id: String,
        clients: usize,
        requests: u64,
        get_ratio: f64,
        keys: u64,
        value_size: usize,
    },
    LoadDone {
        id: String,
        report: Report,
        error: Option<String>,
    },
    ExperimentPhase { name: String, detail: String },
    ExperimentDone {
        name: String,
        result: serde_json::Value,
    },
    Info { message: String },
}