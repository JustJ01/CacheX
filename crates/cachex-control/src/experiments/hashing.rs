

use std::sync::Arc;
use std::time::Instant;

use cachex_core::hashing::{moved_fraction, ConsistentHasher, ModuloHasher};
use serde::Deserialize;

use crate::events::Event;
use crate::state::AppState;

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct HashingParams {
    pub keys: u64,
    pub vnodes: usize,
    pub seed: u64,
}

pub async fn run_hashing(state: &Arc<AppState>, params: HashingParams) -> serde_json::Value {
    state.emit(Event::ExperimentPhase {
        name: "hashing".to_string(),
        detail: format!(
            "computing moved fraction over {} keys (vnodes {})",
            params.keys, params.vnodes
        ),
    });

    let started = Instant::now();
    let keys: Vec<String> = (0..params.keys)
        .map(|i| format!("key:{:08}", (i as u64).wrapping_add(params.seed)))
        .collect();

    let three: Vec<String> = ["127.0.0.1:7001", "127.0.0.1:7002", "127.0.0.1:7003"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let four: Vec<String> = [
        "127.0.0.1:7001",
        "127.0.0.1:7002",
        "127.0.0.1:7003",
        "127.0.0.1:7004",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    let modulo_before = ModuloHasher::new(three.clone());
    let modulo_after = ModuloHasher::new(four.clone());
    let modulo_moved = moved_fraction(&modulo_before, &modulo_after, &keys);

    let consistent_before = ConsistentHasher::new(three, params.vnodes.max(1));
    let consistent_after = ConsistentHasher::new(four, params.vnodes.max(1));
    let consistent_moved = moved_fraction(&consistent_before, &consistent_after, &keys);

    
    let load: Vec<serde_json::Value> = {
        let spec = state.spec.lock().unwrap().clone();
        let mut out = Vec::new();
        for node_id in 1..=spec.node_count {
            let rate = crate::metrics::metrics_of(state, node_id)
                .await
                .map(|s| s.rates.total)
                .unwrap_or(0);
            out.push(serde_json::json!({
                "node": spec.public_address(node_id),
                "req_per_s": rate,
            }));
        }
        out
    };

    let result = serde_json::json!({
        "keys": params.keys,
        "vnodes": params.vnodes,
        "seed": params.seed,
        "elapsed_ms": started.elapsed().as_millis(),
        "modulo_moved": modulo_moved,
        "consistent_moved": consistent_moved,
        "modulo_moved_pct": (modulo_moved * 100.0),
        "consistent_moved_pct": (consistent_moved * 100.0),
        "live_load": load,
    });

    state.emit(Event::ExperimentDone {
        name: "hashing".to_string(),
        result: result.clone(),
    });
    *state.last_experiment.lock().unwrap() = Some(result.clone());
    result
}