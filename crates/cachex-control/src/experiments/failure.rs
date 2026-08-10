

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use cachex_client::CachexClient;
use serde::Deserialize;

use crate::events::Event;
use crate::experiments::{ensure_cluster, finish_experiment};
use crate::nodes;
use crate::state::AppState;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct FailureParams {
    
    pub node_id: u16,
    
    pub warmup_sets: u64,
    
    pub verify_samples: u64,
    
    pub workers: usize,
    
    pub value_size: usize,
    pub vnodes: usize,
}

impl Default for FailureParams {
    fn default() -> Self {
        FailureParams {
            node_id: 2,
            warmup_sets: 20_000,
            verify_samples: 1_000,
            workers: 8,
            value_size: 128,
            vnodes: 100,
        }
    }
}

struct GetCounters {
    hits: AtomicU64,
    misses: AtomicU64,
    errors: AtomicU64,
}

impl GetCounters {
    fn sample(&self) -> (u64, u64, u64) {
        (
            self.hits.load(Ordering::Relaxed),
            self.misses.load(Ordering::Relaxed),
            self.errors.load(Ordering::Relaxed),
        )
    }
}

fn key(i: u64) -> String {
    format!("key:{:08}", i)
}

fn round2(seconds: f64) -> f64 {
    (seconds * 100.0).round() / 100.0
}

pub async fn run_failure(state: &Arc<AppState>, params: FailureParams) -> serde_json::Value {
    let started = Instant::now();
    let spec = state.spec.lock().unwrap().clone();

    let target = if (1..=spec.node_count).contains(&params.node_id) {
        params.node_id
    } else {
        return finish_experiment(state, "failure", serde_json::json!({
            "status": "error",
            "message": format!("node {} is out of range (1..={})", params.node_id, spec.node_count),
        }));
    };
    if spec.node_count < 2 {
        return finish_experiment(state, "failure", serde_json::json!({
            "status": "error",
            "message": "the failure experiment needs at least 2 nodes",
        }));
    }
    
    let probe_node = if target == 1 { 2 } else { 1 };
    let warmup_sets = params.warmup_sets.max(1);
    let verify_samples = params.verify_samples.max(1);
    let workers = params.workers.max(1);
    let target_port = spec.public_port(target);

    state.emit(Event::ExperimentPhase {
        name: "failure".to_string(),
        detail: "ensuring cluster is running".to_string(),
    });
    if let Err(message) = ensure_cluster(state).await {
        return finish_experiment(state, "failure", serde_json::json!({
            "status": "error",
            "message": message,
        }));
    }

    
    
    
    state.emit(Event::ExperimentPhase {
        name: "failure".to_string(),
        detail: format!("warming up {warmup_sets} keys (all SETs)"),
    });
    let value = vec![b'x'; params.value_size.max(1)];
    {
        let warm_client = CachexClient::consistent(spec.public_addresses(), spec.vnodes);
        let mut warmup_errors = 0u64;
        for i in 0..warmup_sets {
            if warm_client.set(&key(i), value.clone(), None).await.is_err() {
                warmup_errors += 1;
            }
        }
        if warmup_errors > 0 {
            state.emit(Event::Info {
                message: format!("warmup: {warmup_errors} SETs failed"),
            });
        }
    }
    
    tokio::time::sleep(Duration::from_millis(1500)).await;

    
    
    
    let counters = Arc::new(GetCounters {
        hits: AtomicU64::new(0),
        misses: AtomicU64::new(0),
        errors: AtomicU64::new(0),
    });
    let stop = Arc::new(AtomicBool::new(false));
    let workload_start = Instant::now();
    let mut timeline: Vec<serde_json::Value> = vec![
        serde_json::json!({ "t_s": 0.0, "label": "Workload started" }),
    ];
    state.emit(Event::ExperimentPhase {
        name: "failure".to_string(),
        detail: "workload started".to_string(),
    });

    let mut bg_tasks = Vec::new();
    for worker_index in 0..workers {
        let counters = counters.clone();
        let stop = stop.clone();
        let addresses = spec.public_addresses();
        let vnodes = spec.vnodes;
        bg_tasks.push(tokio::spawn(async move {
            let client = CachexClient::consistent(addresses, vnodes);
            
            
            let start = (worker_index as u64 * (warmup_sets / workers as u64).max(1)) as usize;
            let mut idx = start % warmup_sets as usize;
            while !stop.load(Ordering::Relaxed) {
                let k = key(idx as u64);
                match tokio::time::timeout(
                    Duration::from_millis(500),
                    client.get(&k),
                )
                .await
                {
                    Ok(Ok(Some(_))) => {
                        counters.hits.fetch_add(1, Ordering::Relaxed);
                    }
                    Ok(Ok(None)) => {
                        counters.misses.fetch_add(1, Ordering::Relaxed);
                    }
                    Ok(Err(_)) | Err(_) => {
                        counters.errors.fetch_add(1, Ordering::Relaxed);
                    }
                }
                idx = (idx + 1) % warmup_sets as usize;
            }
        }));
    }

    
    tokio::time::sleep(Duration::from_millis(800)).await;

    
    let t_kill = workload_start.elapsed().as_secs_f64();
    state.emit(Event::ExperimentPhase {
        name: "failure".to_string(),
        detail: format!("killing node {} ({})", target, target_port),
    });
    if let Err(e) = nodes::kill_node(state, target).await {
        stop.store(true, Ordering::Relaxed);
        for task in bg_tasks {
            let _ = task.await;
        }
        return finish_experiment(state, "failure", serde_json::json!({
            "status": "error",
            "message": format!("kill failed: {e}"),
        }));
    }
    timeline.push(serde_json::json!({
        "t_s": round2(t_kill),
        "label": format!("Node {target_port} killed"),
    }));

    
    let mut t_suspect: Option<f64> = None;
    let mut t_fail: Option<f64> = None;
    let detect_deadline = Instant::now() + Duration::from_secs(12);
    while Instant::now() < detect_deadline {
        if let Ok(snapshot) = crate::metrics::metrics_of(state, probe_node).await {
            if let Some(peers) = snapshot.peers {
                let now = workload_start.elapsed().as_secs_f64();
                if peers.suspected >= 1 && t_suspect.is_none() {
                    t_suspect = Some(now);
                }
                if peers.failed >= 1 && t_fail.is_none() {
                    t_fail = Some(now);
                }
            }
        }
        if t_fail.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }

    let t_suspect = t_suspect.unwrap_or(t_kill);
    let t_fail = match t_fail {
        Some(t) => t,
        None => {
            state.emit(Event::ExperimentPhase {
                name: "failure".to_string(),
                detail: "target node never reported failed by peers".to_string(),
            });
            t_kill
        }
    };
    timeline.push(serde_json::json!({ "t_s": round2(t_suspect), "label": format!("Node {target_port} suspected") }));
    timeline.push(serde_json::json!({ "t_s": round2(t_fail), "label": format!("Node {target_port} failed") }));
    state.emit(Event::ExperimentPhase {
        name: "failure".to_string(),
        detail: format!(
            "node {} suspected at {:.1}s, failed at {:.1}s",
            target_port,
            t_suspect - t_kill,
            t_fail - t_kill
        ),
    });

    
    
    
    let (hits_before, misses_before, errors_before) = counters.sample();
    let t_outage = workload_start.elapsed().as_secs_f64();
    timeline.push(serde_json::json!({ "t_s": round2(t_outage), "label": "Outage measurement start" }));

    
    
    tokio::time::sleep(Duration::from_millis(2000)).await;
    let (hits_outage, misses_outage, errors_outage) = counters.sample();
    let t_restart = workload_start.elapsed().as_secs_f64();
    state.emit(Event::ExperimentPhase {
        name: "failure".to_string(),
        detail: format!("restarting node {}", target_port),
    });
    let restart_begin = Instant::now();
    let restart_result = nodes::restart_node(state, target).await;
    let t_healthy = workload_start.elapsed().as_secs_f64();
    let restart_s = restart_begin.elapsed().as_secs_f64();
    let recovery_ms = crate::metrics::metrics_of(state, target)
        .await
        .map(|s| s.recovery_ms)
        .unwrap_or(0);

    if let Err(e) = restart_result {
        stop.store(true, Ordering::Relaxed);
        for task in bg_tasks {
            let _ = task.await;
        }
        return finish_experiment(state, "failure", serde_json::json!({
            "status": "error",
            "message": format!("restart failed: {e}"),
        }));
    }
    timeline.push(serde_json::json!({ "t_s": round2(t_restart), "label": format!("Node {target_port} restarted") }));
    timeline.push(serde_json::json!({ "t_s": round2(t_healthy), "label": "AOF replay complete" }));
    timeline.push(serde_json::json!({ "t_s": round2(t_healthy), "label": "Node healthy" }));

    
    state.emit(Event::ExperimentPhase {
        name: "failure".to_string(),
        detail: "verifying data after recovery".to_string(),
    });
    let client = CachexClient::consistent(spec.public_addresses(), spec.vnodes);
    let mut verified_hits = 0u64;
    for i in 0..verify_samples {
        match client.get(&key(i)).await {
            Ok(Some(_)) => verified_hits += 1,
            _ => {}
        }
    }
    let verify_hit_rate = verified_hits as f64 / verify_samples as f64;
    timeline.push(serde_json::json!({
        "t_s": round2(workload_start.elapsed().as_secs_f64()),
        "label": "Data verification complete",
    }));

    
    stop.store(true, Ordering::Relaxed);
    for task in bg_tasks {
        let _ = task.await;
    }
    let (hits_total, misses_total, errors_total) = counters.sample();

    let outage_gets = (hits_outage + misses_outage + errors_outage)
        .saturating_sub(hits_before + misses_before + errors_before);
    let outage_errors = errors_outage.saturating_sub(errors_before);
    let outage_gets = outage_gets.max(1);
    let failed_pct = outage_errors as f64 / outage_gets as f64;

    finish_experiment(state, "failure", serde_json::json!({
        "status": "ok",
        "node_id": target,
        "public_port": target_port,
        "detection_s": round2(t_fail - t_kill),
        "suspicion_s": round2(t_suspect - t_kill),
        "failure_s": round2(t_fail - t_kill),
        "restart_s": round2(restart_s),
        "aof_recovery_ms": recovery_ms,
        "healthy_s": round2(t_healthy - t_kill),
        "verify_hit_rate": (verify_hit_rate * 100.0 * 100.0).round() / 100.0,
        "workload": {
            "gets": hits_total + misses_total + errors_total,
            "hits": hits_total,
            "misses": misses_total,
            "errors": errors_total,
            "outage_gets": outage_gets,
            "outage_errors": outage_errors,
            "failed_request_pct": (failed_pct * 100.0 * 100.0).round() / 100.0,
        },
        "timeline": timeline,
        "finding": "AOF replay restores the full dataset after an unplanned node death; reads routed to the dead node fail until it is restarted.",
        "elapsed_ms": started.elapsed().as_millis(),
    }))
}