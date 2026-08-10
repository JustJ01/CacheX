

use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub node: NodeConfig,
    pub cluster: ClusterConfig,
    pub cache: CacheConfig,
    pub aof: AofConfig,
    pub hashing: HashingConfig,
    pub replication: ReplicationConfig,
    pub heartbeat: HeartbeatConfig,
    pub metrics: MetricsConfig,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            node: NodeConfig::default(),
            cluster: ClusterConfig::default(),
            cache: CacheConfig::default(),
            aof: AofConfig::default(),
            hashing: HashingConfig::default(),
            replication: ReplicationConfig::default(),
            heartbeat: HeartbeatConfig::default(),
            metrics: MetricsConfig::default(),
        }
    }
}

#[derive(Debug)]
pub enum ConfigError {
    Io(std::io::Error),
    Toml(toml::de::Error),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Io(e) => write!(f, "failed to read config: {e}"),
            ConfigError::Toml(e) => write!(f, "failed to parse config: {e}"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl Config {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let text = fs::read_to_string(path).map_err(ConfigError::Io)?;
        toml::from_str(&text).map_err(ConfigError::Toml)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NodeConfig {
    pub id: u32,
    pub host: String,
    pub port: u16,
}

impl Default for NodeConfig {
    fn default() -> Self {
        NodeConfig {
            id: 1,
            host: "127.0.0.1".to_string(),
            port: 7001,
        }
    }
}

impl NodeConfig {
    pub fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClusterConfig {
    pub nodes: Vec<String>,
}

impl Default for ClusterConfig {
    fn default() -> Self {
        ClusterConfig { nodes: Vec::new() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CacheConfig {
    
    pub max_memory_bytes: u64,
    
    pub eviction_policy: String,
    
    pub ttl_purge_interval_secs: u64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        CacheConfig {
            max_memory_bytes: 100 * 1024 * 1024,
            eviction_policy: "lru".to_string(),
            ttl_purge_interval_secs: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AofConfig {
    pub enabled: bool,
    pub path: String,
    
    pub fsync: String,
    pub fsync_interval_secs: u64,
    
    
    pub rewrite_threshold_bytes: u64,
}

impl Default for AofConfig {
    fn default() -> Self {
        AofConfig {
            enabled: false,
            path: "cachex.aof".to_string(),
            fsync: "interval".to_string(),
            fsync_interval_secs: 1,
            rewrite_threshold_bytes: 32 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HashingConfig {
    
    pub vnodes: usize,
}

impl Default for HashingConfig {
    fn default() -> Self {
        HashingConfig { vnodes: 100 }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReplicationConfig {
    pub enabled: bool,
    
    pub factor: u32,
}

impl Default for ReplicationConfig {
    fn default() -> Self {
        ReplicationConfig {
            enabled: false,
            factor: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HeartbeatConfig {
    
    pub interval_secs: u64,
    
    pub timeout_ms: u64,
    
    pub miss_threshold: u32,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        HeartbeatConfig {
            interval_secs: 1,
            timeout_ms: 500,
            miss_threshold: 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MetricsConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        MetricsConfig {
            enabled: false,
            host: "127.0.0.1".to_string(),
            port: 9001,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_config_parses() {
        let text = r#"
[node]
id = 2
host = "127.0.0.1"
port = 7002

[cluster]
nodes = ["127.0.0.1:7001", "127.0.0.1:7002", "127.0.0.1:7003"]

[cache]
max_memory_bytes = 524288000
eviction_policy = "lru"
ttl_purge_interval_secs = 2

[aof]
enabled = true
path = "node2.aof"
fsync = "always"
fsync_interval_secs = 1
rewrite_threshold_bytes = 1048576

[metrics]
enabled = true
host = "127.0.0.1"
port = 9002
"#;
        let config: Config = toml::from_str(text).unwrap();
        assert_eq!(config.node.id, 2);
        assert_eq!(config.node.address(), "127.0.0.1:7002");
        assert_eq!(config.cluster.nodes.len(), 3);
        assert_eq!(config.cache.max_memory_bytes, 524288000);
        assert_eq!(config.aof.fsync, "always");
        assert_eq!(config.aof.rewrite_threshold_bytes, 1048576);
        assert_eq!(config.metrics.enabled, true);
        assert_eq!(config.metrics.port, 9002);
    }

    #[test]
    fn partial_config_uses_defaults() {
        let text = "[node]\nid = 3\n";
        let config: Config = toml::from_str(text).unwrap();
        assert_eq!(config.node.port, 7001);
        assert_eq!(config.cache.max_memory_bytes, 100 * 1024 * 1024);
        assert_eq!(config.aof.enabled, false);
        assert_eq!(config.aof.fsync, "interval");
        assert_eq!(config.hashing.vnodes, 100);
        assert_eq!(config.replication.enabled, false);
        assert_eq!(config.heartbeat.miss_threshold, 2);
        assert_eq!(config.metrics.enabled, false);
        assert_eq!(config.metrics.port, 9001);
    }

    #[test]
    fn default_config_round_trips() {
        let config = Config::default();
        let text = toml::to_string(&config).unwrap();
        let parsed: Config = toml::from_str(&text).unwrap();
        assert_eq!(config, parsed);
    }
}