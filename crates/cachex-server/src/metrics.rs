

use crate::aof::Aof;
use crate::heartbeat::Heartbeat;
use crate::latency::Histogram;
use crate::storage::CacheStore;
use cachex_core::protocol::{Command, Response};
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKind {
    Ping,
    Get,
    Set,
    Delete,
    Info,
}

impl CommandKind {
    pub fn of(command: &Command) -> CommandKind {
        match command {
            Command::Ping => CommandKind::Ping,
            Command::Get { .. } => CommandKind::Get,
            Command::Set { .. } => CommandKind::Set,
            Command::Delete { .. } => CommandKind::Delete,
            Command::Info => CommandKind::Info,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            CommandKind::Ping => "ping",
            CommandKind::Get => "get",
            CommandKind::Set => "set",
            CommandKind::Delete => "delete",
            CommandKind::Info => "info",
        }
    }
}

pub struct Metrics {
    pub total_requests: AtomicU64,
    pub get_requests: AtomicU64,
    pub set_requests: AtomicU64,
    pub delete_requests: AtomicU64,
    pub hits: AtomicU64,
    pub misses: AtomicU64,
    
    pub recovery_ms: AtomicU64,
    pub replication_sent: AtomicU64,
    pub replication_failed: AtomicU64,
    pub replication_received: AtomicU64,
    
    latencies: [Histogram; 5],
    
    rates_total: AtomicU64,
    rates_get: AtomicU64,
    rates_set: AtomicU64,
    rates_delete: AtomicU64,
    
    prev_total: AtomicU64,
    prev_get: AtomicU64,
    prev_set: AtomicU64,
    prev_delete: AtomicU64,
    started: Instant,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    pub fn new() -> Self {
        Metrics {
            total_requests: AtomicU64::new(0),
            get_requests: AtomicU64::new(0),
            set_requests: AtomicU64::new(0),
            delete_requests: AtomicU64::new(0),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            recovery_ms: AtomicU64::new(0),
            replication_sent: AtomicU64::new(0),
            replication_failed: AtomicU64::new(0),
            replication_received: AtomicU64::new(0),
            latencies: std::array::from_fn(|_| Histogram::new()),
            rates_total: AtomicU64::new(0),
            rates_get: AtomicU64::new(0),
            rates_set: AtomicU64::new(0),
            rates_delete: AtomicU64::new(0),
            prev_total: AtomicU64::new(0),
            prev_get: AtomicU64::new(0),
            prev_set: AtomicU64::new(0),
            prev_delete: AtomicU64::new(0),
            started: Instant::now(),
        }
    }

    pub fn set_recovery_ms(&self, ms: u64) {
        self.recovery_ms.store(ms, Ordering::Relaxed);
    }

    pub fn record_request(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_get(&self) {
        self.get_requests.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_set(&self) {
        self.set_requests.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_delete(&self) {
        self.delete_requests.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_hit(&self) {
        self.hits.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_miss(&self) {
        self.misses.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_replication(&self, sent: u64, failed: u64) {
        self.replication_sent.fetch_add(sent, Ordering::Relaxed);
        self.replication_failed.fetch_add(failed, Ordering::Relaxed);
    }

    pub fn record_replication_received(&self) {
        self.replication_received.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_replication_failed(&self) {
        self.replication_failed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_latency(&self, kind: CommandKind, elapsed: Duration) {
        let us = (elapsed.as_micros() as u128).min(u64::MAX as u128) as u64;
        self.latency(kind).record_us(us);
    }

    fn latency(&self, kind: CommandKind) -> &Histogram {
        match kind {
            CommandKind::Ping => &self.latencies[0],
            CommandKind::Get => &self.latencies[1],
            CommandKind::Set => &self.latencies[2],
            CommandKind::Delete => &self.latencies[3],
            CommandKind::Info => &self.latencies[4],
        }
    }

    pub fn uptime_secs(&self) -> u64 {
        self.started.elapsed().as_secs()
    }

    
    
    pub fn sample_rates(&self) {
        Metrics::sample(&self.total_requests, &self.rates_total, &self.prev_total);
        Metrics::sample(&self.get_requests, &self.rates_get, &self.prev_get);
        Metrics::sample(&self.set_requests, &self.rates_set, &self.prev_set);
        Metrics::sample(&self.delete_requests, &self.rates_delete, &self.prev_delete);
    }

    fn sample(counter: &AtomicU64, rate: &AtomicU64, prev: &AtomicU64) {
        let current = counter.load(Ordering::Relaxed);
        let last = prev.swap(current, Ordering::Relaxed);
        rate.store(current.saturating_sub(last), Ordering::Relaxed);
    }

    
    pub fn snapshot(
        &self,
        node: &str,
        store: &CacheStore,
        aof: Option<&Aof>,
        heartbeat: Option<&Heartbeat>,
    ) -> Snapshot {
        let (keys, used_bytes, max_bytes) = store.stats();
        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);
        let hit_rate = if hits + misses == 0 {
            0.0
        } else {
            hits as f64 / (hits + misses) as f64
        };
        let aof_snapshot = aof.map(|aof| AofSnapshot {
            bytes_written: aof.bytes_written(),
            write_count: aof.write_count(),
            fsync_count: aof.fsync_count(),
            rewrite_count: aof.rewrite_count(),
        });
        let peers_snapshot = heartbeat.map(|hb| {
            let (alive, suspected, failed) = hb.summary();
            PeersSnapshot {
                alive,
                suspected,
                failed,
            }
        });
        Snapshot {
            node: node.to_string(),
            uptime_secs: self.uptime_secs(),
            recovery_ms: self.recovery_ms.load(Ordering::Relaxed),
            requests: RequestsSnapshot {
                total: self.total_requests.load(Ordering::Relaxed),
                gets: self.get_requests.load(Ordering::Relaxed),
                sets: self.set_requests.load(Ordering::Relaxed),
                deletes: self.delete_requests.load(Ordering::Relaxed),
                hits,
                misses,
                hit_rate,
            },
            rates: RatesSnapshot {
                total: self.rates_total.load(Ordering::Relaxed),
                get: self.rates_get.load(Ordering::Relaxed),
                set: self.rates_set.load(Ordering::Relaxed),
                delete: self.rates_delete.load(Ordering::Relaxed),
            },
            latency: LatencySnapshot {
                ping: self.summary(CommandKind::Ping),
                get: self.summary(CommandKind::Get),
                set: self.summary(CommandKind::Set),
                delete: self.summary(CommandKind::Delete),
                info: self.summary(CommandKind::Info),
            },
            storage: StorageSnapshot {
                keys: keys as u64,
                used_bytes,
                max_bytes,
                evictions: store.eviction_count(),
                ttl_expirations: store.ttl_expiration_count(),
            },
            aof: aof_snapshot,
            replication: ReplicationSnapshot {
                sent: self.replication_sent.load(Ordering::Relaxed),
                failed: self.replication_failed.load(Ordering::Relaxed),
                received: self.replication_received.load(Ordering::Relaxed),
            },
            peers: peers_snapshot,
        }
    }

    fn summary(&self, kind: CommandKind) -> LatencySummary {
        let h = self.latency(kind);
        LatencySummary {
            count: h.count(),
            avg_us: h.avg_us(),
            p50_us: h.percentile_us(50.0),
            p95_us: h.percentile_us(95.0),
            p99_us: h.percentile_us(99.0),
            max_us: h.max_us(),
        }
    }

    
    pub fn info(
        &self,
        store: &CacheStore,
        aof: Option<&Aof>,
        heartbeat: Option<&Heartbeat>,
    ) -> Response {
        let (keys, used_bytes, max_bytes) = store.stats();
        let aof_text = match aof {
            Some(aof) => format!(
                "aof_bytes={} aof_writes={} aof_fsyncs={} aof_rewrites={}",
                aof.bytes_written(),
                aof.write_count(),
                aof.fsync_count(),
                aof.rewrite_count(),
            ),
            None => "aof=off".to_string(),
        };
        let heartbeat_text = match heartbeat {
            Some(hb) => {
                let (alive, suspected, failed) = hb.summary();
                format!("peers_alive={alive} peers_suspected={suspected} peers_failed={failed}")
            }
            None => "peers=off".to_string(),
        };
        let set_latency = self.latency(CommandKind::Set);
        let get_latency = self.latency(CommandKind::Get);
        let text = format!(
            "keys={keys} used_bytes={used_bytes} max_bytes={max_bytes} \
             requests={} gets={} sets={} deletes={} hits={} misses={} \
             evictions={} ttl_expirations={} recovery_ms={} uptime_secs={} \
             req_per_s={} get_per_s={} set_per_s={} del_per_s={} \
             set_p99_us={} get_p99_us={} \
             replication_sent={} replication_failed={} replication_received={} {aof_text} {heartbeat_text}",
            self.total_requests.load(Ordering::Relaxed),
            self.get_requests.load(Ordering::Relaxed),
            self.set_requests.load(Ordering::Relaxed),
            self.delete_requests.load(Ordering::Relaxed),
            self.hits.load(Ordering::Relaxed),
            self.misses.load(Ordering::Relaxed),
            store.eviction_count(),
            store.ttl_expiration_count(),
            self.recovery_ms.load(Ordering::Relaxed),
            self.uptime_secs(),
            self.rates_total.load(Ordering::Relaxed),
            self.rates_get.load(Ordering::Relaxed),
            self.rates_set.load(Ordering::Relaxed),
            self.rates_delete.load(Ordering::Relaxed),
            set_latency.percentile_us(99.0),
            get_latency.percentile_us(99.0),
            self.replication_sent.load(Ordering::Relaxed),
            self.replication_failed.load(Ordering::Relaxed),
            self.replication_received.load(Ordering::Relaxed),
        );
        Response::Info(text)
    }
}

#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub node: String,
    pub uptime_secs: u64,
    pub recovery_ms: u64,
    pub requests: RequestsSnapshot,
    pub rates: RatesSnapshot,
    pub latency: LatencySnapshot,
    pub storage: StorageSnapshot,
    pub aof: Option<AofSnapshot>,
    pub replication: ReplicationSnapshot,
    pub peers: Option<PeersSnapshot>,
}

#[derive(Debug, Serialize)]
pub struct RequestsSnapshot {
    pub total: u64,
    pub gets: u64,
    pub sets: u64,
    pub deletes: u64,
    pub hits: u64,
    pub misses: u64,
    pub hit_rate: f64,
}

#[derive(Debug, Serialize)]
pub struct RatesSnapshot {
    pub total: u64,
    pub get: u64,
    pub set: u64,
    pub delete: u64,
}

#[derive(Debug, Serialize)]
pub struct LatencySnapshot {
    pub ping: LatencySummary,
    pub get: LatencySummary,
    pub set: LatencySummary,
    pub delete: LatencySummary,
    pub info: LatencySummary,
}

#[derive(Debug, Serialize)]
pub struct LatencySummary {
    pub count: u64,
    pub avg_us: u64,
    pub p50_us: u64,
    pub p95_us: u64,
    pub p99_us: u64,
    pub max_us: u64,
}

#[derive(Debug, Serialize)]
pub struct StorageSnapshot {
    pub keys: u64,
    pub used_bytes: u64,
    pub max_bytes: u64,
    pub evictions: u64,
    pub ttl_expirations: u64,
}

#[derive(Debug, Serialize)]
pub struct AofSnapshot {
    pub bytes_written: u64,
    pub write_count: u64,
    pub fsync_count: u64,
    pub rewrite_count: u64,
}

#[derive(Debug, Serialize)]
pub struct ReplicationSnapshot {
    pub sent: u64,
    pub failed: u64,
    pub received: u64,
}

#[derive(Debug, Serialize)]
pub struct PeersSnapshot {
    pub alive: usize,
    pub suspected: usize,
    pub failed: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::CacheStore;

    fn store() -> CacheStore {
        CacheStore::new(1_000_000)
    }

    #[test]
    fn snapshot_reflects_counters() {
        let metrics = Metrics::new();
        let store = store();
        metrics.record_request();
        metrics.record_set();
        metrics.record_get();
        metrics.record_hit();
        metrics.record_replication(1, 0);

        let snap = metrics.snapshot("127.0.0.1:7001", &store, None, None);
        assert_eq!(snap.node, "127.0.0.1:7001");
        assert_eq!(snap.requests.total, 1);
        assert_eq!(snap.requests.sets, 1);
        assert_eq!(snap.requests.gets, 1);
        assert_eq!(snap.requests.hit_rate, 1.0);
        assert_eq!(snap.replication.sent, 1);
        assert!(snap.aof.is_none());
        assert!(snap.peers.is_none());
    }

    #[test]
    fn sample_rates_computes_deltas() {
        let metrics = Metrics::new();
        for _ in 0..10 {
            metrics.record_request();
        }
        metrics.sample_rates();
        assert_eq!(metrics.rates_total.load(Ordering::Relaxed), 10);

        
        metrics.sample_rates();
        assert_eq!(metrics.rates_total.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn record_latency_feeds_histogram() {
        let metrics = Metrics::new();
        metrics.record_latency(CommandKind::Get, Duration::from_micros(123));
        let summary = metrics.summary(CommandKind::Get);
        assert_eq!(summary.count, 1);
        assert_eq!(summary.avg_us, 123);
        assert_eq!(summary.max_us, 123);
        assert_eq!(metrics.summary(CommandKind::Set).count, 0);
    }

    #[test]
    fn command_kind_classification() {
        use cachex_core::protocol::Command;
        assert_eq!(CommandKind::of(&Command::Ping), CommandKind::Ping);
        assert_eq!(
            CommandKind::of(&Command::Set { key: "k".into(), value: vec![], ttl: None }),
            CommandKind::Set
        );
        assert_eq!(
            CommandKind::of(&Command::Get { key: "k".into() }),
            CommandKind::Get
        );
        assert_eq!(CommandKind::of(&Command::Info), CommandKind::Info);
    }

    #[test]
    fn info_line_includes_rates_and_latency() {
        let metrics = Metrics::new();
        let store = store();
        metrics.record_request();
        metrics.record_set();
        metrics.record_latency(CommandKind::Set, Duration::from_micros(50));
        metrics.sample_rates();

        let Response::Info(text) = metrics.info(&store, None, None) else {
            panic!("expected INFO response");
        };
        assert!(text.contains("req_per_s="), "missing rate: {text}");
        assert!(text.contains("set_p99_us="), "missing latency: {text}");
    }
}