

pub mod cli;
pub mod report;
pub mod workload;

use crate::cli::{Config, RouterKind};
use crate::report::Report;
use crate::workload::Workload;
use cachex_client::{CachexClient, ClientError};
use cachex_core::hashing::{ConsistentHasher, ModuloHasher};
use cachex_core::latency::Histogram;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

const SEED_STRIDE: u64 = 0x9E37_79B9_7F4A_7C15;

struct SharedStats {
    histogram: Histogram,
    gets: AtomicU64,
    sets: AtomicU64,
    hits: AtomicU64,
    misses: AtomicU64,
    errors: AtomicU64,
}

enum BenchClient {
    Consistent(CachexClient<ConsistentHasher>),
    Modulo(CachexClient<ModuloHasher>),
}

impl BenchClient {
    fn new(config: &Config) -> Self {
        match config.router {
            RouterKind::Consistent => {
                BenchClient::Consistent(CachexClient::consistent(config.nodes.clone(), config.vnodes))
            }
            RouterKind::Modulo => BenchClient::Modulo(CachexClient::modulo(config.nodes.clone())),
        }
    }

    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, ClientError> {
        match self {
            BenchClient::Consistent(client) => client.get(key).await,
            BenchClient::Modulo(client) => client.get(key).await,
        }
    }

    async fn set(&self, key: &str, value: Vec<u8>, ttl: Option<u64>) -> Result<(), ClientError> {
        match self {
            BenchClient::Consistent(client) => client.set(key, value, ttl).await,
            BenchClient::Modulo(client) => client.set(key, value, ttl).await,
        }
    }
}

async fn worker(
    client: BenchClient,
    mut workload: Workload,
    count: u64,
    ttl: Option<u64>,
    shared: Arc<SharedStats>,
) {
    for _ in 0..count {
        let key = workload.next_key();
        let start = Instant::now();
        if workload.should_get() {
            shared.gets.fetch_add(1, Ordering::Relaxed);
            match client.get(&key).await {
                Ok(Some(_)) => {
                    shared.hits.fetch_add(1, Ordering::Relaxed);
                }
                Ok(None) => {
                    shared.misses.fetch_add(1, Ordering::Relaxed);
                }
                Err(_) => {
                    shared.errors.fetch_add(1, Ordering::Relaxed);
                }
            }
        } else {
            shared.sets.fetch_add(1, Ordering::Relaxed);
            let value = workload.next_value();
            if client.set(&key, value, ttl).await.is_err() {
                shared.errors.fetch_add(1, Ordering::Relaxed);
            }
        }
        shared.histogram.record_us(start.elapsed().as_micros() as u64);
    }
}

pub async fn run(config: Config) -> Result<Report, String> {
    let report = run_report(&config).await?;

    println!("{}", report.summary());
    let csv = format!("{}\n{}\n", Report::header(), report.csv_row());
    std::fs::write(&config.output, csv)
        .map_err(|error| format!("failed to write {}: {error}", config.output))?;
    println!("results written to {}", config.output);
    Ok(report)
}

pub async fn run_report(config: &Config) -> Result<Report, String> {
    let started = Instant::now();
    let shared = Arc::new(SharedStats {
        histogram: Histogram::new(),
        gets: AtomicU64::new(0),
        sets: AtomicU64::new(0),
        hits: AtomicU64::new(0),
        misses: AtomicU64::new(0),
        errors: AtomicU64::new(0),
    });

    let base_share = config.requests / config.clients as u64;
    let remainder = config.requests % config.clients as u64;
    let ttl = if config.ttl == 0 { None } else { Some(config.ttl) };

    let mut handles = Vec::new();
    for index in 0..config.clients {
        let client = BenchClient::new(config);
        let workload = Workload::new(
            config.keys,
            config.value_size,
            config.get_ratio,
            config.key_order,
            config.seed.wrapping_mul(SEED_STRIDE).wrapping_add(index as u64),
        );
        let my_requests = base_share + u64::from((index as u64) < remainder);
        let shared = shared.clone();
        handles.push(tokio::spawn(async move {
            worker(client, workload, my_requests, ttl, shared).await;
        }));
    }
    for handle in handles {
        handle.await.map_err(|error| format!("worker task failed: {error}"))?;
    }

    let total_secs = started.elapsed().as_secs_f64();
    Ok(Report {
        nodes: config.nodes.len(),
        router: config.router.to_string(),
        vnodes: config.vnodes,
        clients: config.clients,
        requests: config.requests,
        keys: config.keys,
        value_size: config.value_size,
        get_ratio: config.get_ratio,
        seed: config.seed,
        key_order: config.key_order.to_string(),
        total_secs,
        ops_per_sec: if total_secs > 0.0 {
            config.requests as f64 / total_secs
        } else {
            0.0
        },
        gets: shared.gets.load(Ordering::Relaxed),
        sets: shared.sets.load(Ordering::Relaxed),
        hits: shared.hits.load(Ordering::Relaxed),
        misses: shared.misses.load(Ordering::Relaxed),
        errors: shared.errors.load(Ordering::Relaxed),
        avg_us: shared.histogram.avg_us(),
        p50_us: shared.histogram.percentile_us(50.0),
        p95_us: shared.histogram.percentile_us(95.0),
        p99_us: shared.histogram.percentile_us(99.0),
        max_us: shared.histogram.max_us(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workload::KeyOrder;
    use cachex_server::{metrics::Metrics, node::NodeContext, server, storage::CacheStore};
    use tokio::net::TcpListener;

    async fn start_node() -> String {
        let store = Arc::new(CacheStore::new(1_000_000));
        let metrics = Arc::new(Metrics::new());
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let ctx = NodeContext::standalone(store, metrics, None, address.clone());
        tokio::spawn(async move {
            let _ = server::run(listener, ctx).await;
        });
        address
    }

    fn shared_stats() -> Arc<SharedStats> {
        Arc::new(SharedStats {
            histogram: Histogram::new(),
            gets: AtomicU64::new(0),
            sets: AtomicU64::new(0),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            errors: AtomicU64::new(0),
        })
    }

    #[tokio::test]
    async fn worker_counts_gets_sets_and_misses_against_a_live_node() {
        let node = start_node().await;
        let client = BenchClient::Consistent(CachexClient::consistent(vec![node], 10));
        let shared = shared_stats();

        
        let workload = Workload::new(10, 8, 0.5, KeyOrder::Uniform, 1);
        worker(client, workload, 100, None, shared.clone()).await;

        assert_eq!(shared.histogram.count(), 100);
        let total = shared.gets.load(Ordering::Relaxed) + shared.sets.load(Ordering::Relaxed);
        assert_eq!(total, 100, "every request is either a GET or a SET");
        
        
        let gets = shared.gets.load(Ordering::Relaxed);
        assert_eq!(
            shared.hits.load(Ordering::Relaxed) + shared.misses.load(Ordering::Relaxed),
            gets,
            "every GET is either a hit or a miss"
        );
        assert_eq!(shared.errors.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn worker_records_connection_errors_without_hanging() {
        
        
        let client = BenchClient::Consistent(CachexClient::consistent(
            vec!["127.0.0.1:1".to_string()],
            10,
        ));
        let shared = shared_stats();
        let workload = Workload::new(10, 8, 1.0, KeyOrder::Uniform, 1);

        worker(client, workload, 2, None, shared.clone()).await;

        assert_eq!(shared.gets.load(Ordering::Relaxed), 2);
        assert_eq!(shared.errors.load(Ordering::Relaxed), 2);
        assert_eq!(shared.histogram.count(), 2);
    }
}