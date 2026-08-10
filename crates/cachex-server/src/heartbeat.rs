

use crate::internal::{InternalConnection, InternalMessage, InternalResponse};
use cachex_core::config::HeartbeatConfig;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::time::interval;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerStatus {
    Alive,
    Suspected,
    Failed,
}

impl PeerStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            PeerStatus::Alive => "alive",
            PeerStatus::Suspected => "suspected",
            PeerStatus::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PeerState {
    pub status: PeerStatus,
    pub missed: u32,
    pub last_seen: Instant,
    pub detected_failed_at: Option<Instant>,
}

pub struct Heartbeat {
    peers: Mutex<HashMap<String, PeerState>>,
    peer_addrs: Vec<String>,
    internal: HashMap<String, String>,
    interval: Duration,
    timeout: Duration,
    miss_threshold: u32,
    
    pub last_detection_ns: AtomicU64,
}

impl Heartbeat {
    pub fn new(
        self_address: &str,
        cluster_nodes: &[String],
        internal: HashMap<String, String>,
        config: HeartbeatConfig,
    ) -> Self {
        let peer_addrs: Vec<String> = cluster_nodes
            .iter()
            .filter(|node| *node != self_address)
            .cloned()
            .collect();
        let peers: HashMap<String, PeerState> = peer_addrs
            .iter()
            .map(|peer| {
                (
                    peer.clone(),
                    PeerState {
                        status: PeerStatus::Alive,
                        missed: 0,
                        last_seen: Instant::now(),
                        detected_failed_at: None,
                    },
                )
            })
            .collect();
        Heartbeat {
            peers: Mutex::new(peers),
            peer_addrs,
            internal,
            interval: Duration::from_secs(config.interval_secs.max(1)),
            timeout: Duration::from_millis(config.timeout_ms.max(50)),
            miss_threshold: config.miss_threshold.max(1),
            last_detection_ns: AtomicU64::new(0),
        }
    }

    pub async fn run(self: Arc<Self>) {
        let mut ticker = interval(self.interval);
        loop {
            ticker.tick().await;
            self.tick().await;
        }
    }

    
    pub async fn tick(self: &Arc<Self>) {
        let mut handles = Vec::new();
        for peer in self.peer_addrs.clone() {
            let this = self.clone();
            handles.push(tokio::spawn(async move { this.ping_one(&peer).await }));
        }
        for handle in handles {
            let _ = handle.await;
        }
    }

    async fn ping_one(self: &Arc<Self>, peer: &str) {
        let Some(internal) = self.internal.get(peer).cloned() else {
            return;
        };
        let timeout = self.timeout;
        let result = tokio::time::timeout(timeout, async {
            let mut conn = InternalConnection::connect(&internal).await?;
            conn.request(&InternalMessage::Ping).await
        })
        .await;

        let ok = matches!(result, Ok(Ok(InternalResponse::Pong)));
        if ok {
            self.mark_alive(peer);
        } else {
            self.mark_missed(peer);
        }
    }

    fn mark_alive(&self, peer: &str) {
        let mut peers = self.peers.lock().expect("heartbeat lock poisoned");
        if let Some(state) = peers.get_mut(peer) {
            if state.status != PeerStatus::Alive {
                state.status = PeerStatus::Alive;
                state.missed = 0;
                let now = Instant::now();
                if let Some(detected) = state.detected_failed_at {
                    self.last_detection_ns.store(
                        now.duration_since(detected).as_nanos() as u64,
                        Ordering::Relaxed,
                    );
                }
                state.detected_failed_at = None;
            }
            state.last_seen = Instant::now();
        }
    }

    fn mark_missed(&self, peer: &str) {
        let mut peers = self.peers.lock().expect("heartbeat lock poisoned");
        if let Some(state) = peers.get_mut(peer) {
            state.missed = state.missed.saturating_add(1);
            state.status = if state.missed >= self.miss_threshold {
                if state.detected_failed_at.is_none() {
                    state.detected_failed_at = Some(Instant::now());
                }
                PeerStatus::Failed
            } else if state.missed == 1 {
                PeerStatus::Suspected
            } else {
                state.status
            };
        }
    }

    pub fn status_of(&self, peer: &str) -> Option<PeerStatus> {
        self.peers
            .lock()
            .expect("heartbeat lock poisoned")
            .get(peer)
            .map(|s| s.status)
    }

    
    pub fn summary(&self) -> (usize, usize, usize) {
        let peers = self.peers.lock().expect("heartbeat lock poisoned");
        let alive = peers.values().filter(|s| s.status == PeerStatus::Alive).count();
        let suspected = peers
            .values()
            .filter(|s| s.status == PeerStatus::Suspected)
            .count();
        let failed = peers.values().filter(|s| s.status == PeerStatus::Failed).count();
        (alive, suspected, failed)
    }
}