

use std::sync::Arc;
use std::time::{Duration, Instant};

use cachex_server::storage::CacheStore;
use serde::Deserialize;

use crate::events::Event;
use crate::experiments::finish_experiment;
use crate::state::AppState;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TtlParams {
    pub keys: u64,
    pub ttl_secs: u64,
    pub value_size: usize,
    
    pub interval_secs: u64,
}

impl Default for TtlParams {
    fn default() -> Self {
        TtlParams {
            keys: 5_000,
            ttl_secs: 10,
            value_size: 64,
            interval_secs: 1,
        }
    }
}

fn key(i: u64) -> String {
    format!("key:{:08}", i)
}

fn keyset(count: u64) -> Vec<String> {
    (0..count).map(key).collect()
}

fn probe_hit_rate(store: &CacheStore, set: &[String]) -> f64 {
    let attempts = ((set.len() as u64).saturating_mul(2)).clamp(2_000, 20_000) as usize;
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

pub async fn run_ttl(state: &Arc<AppState>, params: TtlParams) -> serde_json::Value {
    let started = Instant::now();
    let keys = params.keys.max(1);
    let ttl_secs = params.ttl_secs.max(1);
    let interval = params.interval_secs.max(1);
    let value_size = params.value_size.max(1);
    
    
    let store = CacheStore::new(keys.saturating_mul(value_size as u64).saturating_mul(4).max(8 << 20));

    state.emit(Event::ExperimentPhase {
        name: "ttl".to_string(),
        detail: format!("writing {keys} keys with TTL {ttl_secs}s"),
    });

    let set = keyset(keys);
    let value = vec![b'x'; value_size];
    for k in &set {
        store.set(k, value.clone(), Some(ttl_secs));
    }

    let mut samples = Vec::new();
    let mut elapsed = 0u64;
    let mut remaining = store.key_count() as u64;
    loop {
        if elapsed > 0 {
            store.purge_expired();
            remaining = store.key_count() as u64;
        }
        let hit_rate = hit_pct(probe_hit_rate(&store, &set));
        samples.push(serde_json::json!({
            "elapsed_secs": elapsed,
            "remaining": remaining,
            "hit_rate": hit_rate,
        }));
        state.emit(Event::ExperimentPhase {
            name: "ttl".to_string(),
            detail: format!(
                "t={elapsed}s · {remaining}/{keys} keys remaining · {hit_rate}% hit rate"
            ),
        });

        if remaining == 0 || elapsed >= ttl_secs + 2 {
            break;
        }
        elapsed += interval;
        tokio::time::sleep(Duration::from_secs(interval)).await;
    }

    let result = serde_json::json!({
        "status": "ok",
        "keys": keys,
        "ttl_secs": ttl_secs,
        "value_size": value_size,
        "samples": samples,
        "duration_secs": elapsed,
        "total_expired": keys - remaining,
        "elapsed_ms": started.elapsed().as_millis(),
    });

    finish_experiment(state, "ttl", result)
}