

use std::sync::Arc;
use std::time::Instant;

use cachex_server::storage::CacheStore;
use serde::Deserialize;

use crate::events::Event;
use crate::experiments::finish_experiment;
use crate::state::AppState;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct EvictionParams {
    pub capacity_bytes: u64,
    pub working_set: u64,
    pub value_size: usize,
    
    
    pub samples: Vec<u64>,
}

impl Default for EvictionParams {
    fn default() -> Self {
        EvictionParams {
            capacity_bytes: 1024 * 1024,
            working_set: 20_000,
            value_size: 128,
            samples: Vec::new(),
        }
    }
}

fn key(i: u64) -> String {
    format!("key:{:08}", i)
}

fn keyset(count: u64) -> Vec<String> {
    (0..count).map(key).collect()
}

fn fill(store: &CacheStore, count: u64, value: &[u8]) {
    for i in 0..count {
        store.set(&key(i), value.to_vec(), None);
    }
}

fn probe(store: &CacheStore, set: &[String]) -> f64 {
    let attempts = ((set.len() as u64).saturating_mul(3)).clamp(2_000, 60_000) as usize;
    let mut hits = 0u64;
    let mut idx = 0usize;
    let len = set.len().max(1);
    for _ in 0..attempts {
        idx = idx.wrapping_add(0x9E37_79B9_7F4A_7C15) % len;
        if store.get(&set[idx]).0.is_some() {
            hits += 1;
        }
    }
    hits as f64 / attempts as f64
}

fn hit_pct(rate: f64) -> f64 {
    (rate * 100.0 * 100.0).round() / 100.0
}

pub async fn run_eviction(state: &Arc<AppState>, params: EvictionParams) -> serde_json::Value {
    let started = Instant::now();
    let capacity = params.capacity_bytes.max(1);
    let working_set = params.working_set.max(1);
    let value_size = params.value_size.max(1);
    let value = vec![b'x'; value_size];

    state.emit(Event::ExperimentPhase {
        name: "eviction".to_string(),
        detail: format!(
            "capacity {} bytes, working set {} keys",
            capacity, working_set
        ),
    });

    let default_samples: Vec<u64> = [0.05, 0.1, 0.2, 0.35, 0.5, 0.75, 1.0]
        .iter()
        .map(|f| ((working_set as f64) * f).max(1.0) as u64)
        .collect();
    let samples: Vec<u64> = if params.samples.is_empty() {
        default_samples
    } else {
        params
            .samples
            .iter()
            .copied()
            .map(|s| s.min(working_set).max(1))
            .collect()
    };

    
    let mut curve = Vec::new();
    for &count in &samples {
        state.emit(Event::ExperimentPhase {
            name: "eviction".to_string(),
            detail: format!("sampling working set {count} keys"),
        });
        let store = CacheStore::new(capacity);
        fill(&store, count, &value);
        let resident = store.key_count() as u64;
        let hit_rate = hit_pct(probe(&store, &keyset(count)));
        curve.push(serde_json::json!({
            "keys": count,
            "resident": resident,
            "hit_rate": hit_rate,
        }));
    }

    
    
    state.emit(Event::ExperimentPhase {
        name: "eviction".to_string(),
        detail: format!("filling full working set of {working_set} keys"),
    });
    let store = CacheStore::new(capacity);
    fill(&store, working_set, &value);
    let resident_keys = store.key_count() as u64;
    let evictions = store.eviction_count();
    let hit_rate = hit_pct(probe(&store, &keyset(working_set)));

    let result = serde_json::json!({
        "status": "ok",
        "capacity_bytes": capacity,
        "capacity_mb": capacity as f64 / (1024.0 * 1024.0),
        "working_set": working_set,
        "value_size": value_size,
        "resident_keys": resident_keys,
        "evictions": evictions,
        "evicted_pct": hit_pct(if working_set > 0 {
            evictions as f64 / working_set as f64
        } else {
            0.0
        }),
        "hit_rate": hit_rate,
        "curve": curve,
        "elapsed_ms": started.elapsed().as_millis(),
    });

    finish_experiment(state, "eviction", result)
}