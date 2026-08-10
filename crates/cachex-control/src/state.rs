

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

use cachex_bench::report::Report;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::events::Event;
use crate::http;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterSpec {
    pub host: String,
    
    pub public_base: u16,
    
    pub metrics_base: u16,
    pub node_count: u16,
    pub replication_factor: u32,
    pub vnodes: usize,
    pub max_memory_bytes: u64,
}

impl Default for ClusterSpec {
    fn default() -> Self {
        ClusterSpec {
            host: "127.0.0.1".to_string(),
            public_base: 7001,
            metrics_base: 9001,
            node_count: 3,
            replication_factor: 1,
            vnodes: 100,
            max_memory_bytes: 100 * 1024 * 1024,
        }
    }
}

impl ClusterSpec {
    
    pub fn node_for_port(&self, public_port: u16) -> Option<u16> {
        if public_port >= self.public_base && public_port < self.public_base + self.node_count {
            Some(public_port - self.public_base + 1)
        } else {
            None
        }
    }

    pub fn public_port(&self, node_id: u16) -> u16 {
        self.public_base + node_id - 1
    }

    pub fn metrics_port(&self, node_id: u16) -> u16 {
        self.metrics_base + node_id - 1
    }

    
    pub fn internal_port(&self, node_id: u16) -> u16 {
        self.public_port(node_id) + 1000
    }

    pub fn public_address(&self, node_id: u16) -> String {
        format!("{}:{}", self.host, self.public_port(node_id))
    }

    pub fn metrics_url(&self, node_id: u16) -> String {
        format!("http://{}:{}/metrics", self.host, self.metrics_port(node_id))
    }

    pub fn public_addresses(&self) -> Vec<String> {
        (1..=self.node_count).map(|id| self.public_address(id)).collect()
    }
}

#[derive(Debug, Clone)]
pub struct LoadStatus {
    pub id: String,
    pub clients: usize,
    pub requests: u64,
    pub get_ratio: f64,
    pub keys: u64,
    pub value_size: usize,
    pub done: bool,
    pub report: Option<Report>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeInfo {
    pub id: u16,
    pub public_port: u16,
    pub metrics_port: u16,
    pub address: String,
    
    pub pid: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClusterStatus {
    pub spec: ClusterSpec,
    pub nodes: Vec<NodeInfo>,
    
    pub ready: bool,
}

pub struct AppState {
    pub root: PathBuf,
    pub control_dir: PathBuf,
    pub server_exe: PathBuf,
    pub bench_exe: PathBuf,
    pub spec: Mutex<ClusterSpec>,
    
    pub pids: Mutex<HashMap<u16, u32>>,
    
    pub tx: broadcast::Sender<Event>,
    pub loads: Mutex<HashMap<String, LoadStatus>>,
    pub next_load_id: AtomicU64,
    pub last_experiment: Mutex<Option<serde_json::Value>>,
}

impl AppState {
    pub fn new(root: PathBuf, server_exe: PathBuf, bench_exe: PathBuf) -> Arc<Self> {
        let (tx, _) = broadcast::channel(256);
        Arc::new(AppState {
            root: root.clone(),
            control_dir: root.join(".control"),
            server_exe,
            bench_exe,
            spec: Mutex::new(ClusterSpec::default()),
            pids: Mutex::new(HashMap::new()),
            tx,
            loads: Mutex::new(HashMap::new()),
            next_load_id: AtomicU64::new(1),
            last_experiment: Mutex::new(None),
        })
    }

    pub fn emit(&self, event: Event) {
        let _ = self.tx.send(event);
    }

    
    pub async fn node_alive(&self, node_id: u16) -> bool {
        let spec = self.spec.lock().unwrap().clone();
        let port = spec.metrics_port(node_id);
        http::get_json::<serde_json::Value>(&spec.host, port, "/metrics", 1500).await.is_ok()
    }
}