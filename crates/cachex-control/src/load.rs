

use std::sync::atomic::Ordering;
use std::sync::Arc;

use cachex_bench::cli::{Config, RouterKind};
use cachex_bench::workload::KeyOrder;
use serde::Deserialize;

use crate::events::Event;
use crate::state::{AppState, LoadStatus};

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct LoadParams {
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
}

impl Default for LoadParams {
    fn default() -> Self {
        LoadParams {
            clients: 100,
            requests: 10_000,
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

pub fn start_load(state: &Arc<AppState>, params: LoadParams) -> String {
    let id = format!("load-{}", state.next_load_id.fetch_add(1, Ordering::Relaxed));
    {
        let mut loads = state.loads.lock().unwrap();
        loads.insert(
            id.clone(),
            LoadStatus {
                id: id.clone(),
                clients: params.clients,
                requests: params.requests,
                get_ratio: params.get_ratio,
                keys: params.keys,
                value_size: params.value_size,
                done: false,
                report: None,
                error: None,
            },
        );
    }

    state.emit(Event::LoadStarted {
        id: id.clone(),
        clients: params.clients,
        requests: params.requests,
        get_ratio: params.get_ratio,
        keys: params.keys,
        value_size: params.value_size,
    });

    let state = state.clone();
    let task_id = id.clone();
    tokio::spawn(async move {
        let config = build_bench_config(&state, &params);
        let result = cachex_bench::run_report(&config).await;

        let (report, error) = match result {
            Ok(report) => (Some(report.clone()), None),
            Err(message) => (None, Some(message)),
        };

        {
            let mut loads = state.loads.lock().unwrap();
            if let Some(status) = loads.get_mut(&task_id) {
                status.done = true;
                status.report = report.clone();
                status.error = error.clone();
            }
        }

        state.emit(Event::LoadDone {
            id: task_id,
            report: report.unwrap_or_else(|| cachex_bench::report::Report {
                nodes: state.spec.lock().unwrap().node_count as usize,
                router: params.router.clone(),
                vnodes: params.vnodes,
                clients: params.clients,
                requests: params.requests,
                keys: params.keys,
                value_size: params.value_size,
                get_ratio: params.get_ratio,
                seed: params.seed,
                key_order: params.key_order.clone(),
                total_secs: 0.0,
                ops_per_sec: 0.0,
                gets: 0,
                sets: 0,
                hits: 0,
                misses: 0,
                errors: 0,
                avg_us: 0,
                p50_us: 0,
                p95_us: 0,
                p99_us: 0,
                max_us: 0,
            }),
            error,
        });
    });

    id
}

pub fn build_bench_config(state: &Arc<AppState>, params: &LoadParams) -> Config {
    let spec = state.spec.lock().unwrap().clone();
    let key_order = match params.key_order.as_str() {
        "sequential" => KeyOrder::Sequential,
        _ => KeyOrder::Uniform,
    };
    let router = match params.router.as_str() {
        "modulo" => RouterKind::Modulo,
        _ => RouterKind::Consistent,
    };
    Config {
        nodes: spec.public_addresses(),
        router,
        vnodes: params.vnodes,
        clients: params.clients,
        requests: params.requests,
        keys: params.keys,
        value_size: params.value_size,
        get_ratio: params.get_ratio,
        key_order,
        seed: params.seed,
        ttl: params.ttl,
        output: state.control_dir.join("latest-load.csv").to_string_lossy().into_owned(),
    }
}

pub fn load_status(state: &Arc<AppState>, id: &str) -> Option<LoadStatus> {
    state.loads.lock().unwrap().get(id).cloned()
}

pub async fn run_bench(state: &Arc<AppState>, params: &LoadParams) -> Result<cachex_bench::report::Report, String> {
    let config = build_bench_config(state, params);
    cachex_bench::run_report(&config).await
}