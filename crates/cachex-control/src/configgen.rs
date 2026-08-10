

use std::path::Path;

use cachex_core::config::{
    AofConfig, CacheConfig, ClusterConfig, Config, HashingConfig, HeartbeatConfig, MetricsConfig,
    NodeConfig, ReplicationConfig,
};

use crate::state::ClusterSpec;

pub fn build_config(spec: &ClusterSpec, node_id: u16) -> Config {
    let nodes = spec.public_addresses();
    Config {
        node: NodeConfig {
            id: node_id as u32,
            host: spec.host.clone(),
            port: spec.public_port(node_id),
        },
        cluster: ClusterConfig { nodes },
        cache: CacheConfig {
            max_memory_bytes: spec.max_memory_bytes,
            eviction_policy: "lru".to_string(),
            ttl_purge_interval_secs: 1,
        },
        aof: AofConfig {
            enabled: true,
            path: format!("node{node_id}.aof"),
            fsync: "interval".to_string(),
            fsync_interval_secs: 1,
            rewrite_threshold_bytes: 64 * 1024 * 1024,
        },
        hashing: HashingConfig {
            vnodes: spec.vnodes,
        },
        replication: ReplicationConfig {
            enabled: spec.replication_factor > 1,
            factor: spec.replication_factor,
        },
        heartbeat: HeartbeatConfig {
            interval_secs: 1,
            timeout_ms: 500,
            miss_threshold: 2,
        },
        metrics: MetricsConfig {
            enabled: true,
            host: spec.host.clone(),
            port: spec.metrics_port(node_id),
        },
    }
}

pub fn config_path(control_dir: &Path, node_id: u16) -> std::path::PathBuf {
    control_dir.join(format!("node{node_id}.toml"))
}

pub fn write_all_configs(control_dir: &Path, spec: &ClusterSpec) -> std::io::Result<()> {
    std::fs::create_dir_all(control_dir)?;
    for node_id in 1..=spec.node_count {
        let config = build_config(spec, node_id);
        let text = toml::to_string(&config)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        std::fs::write(config_path(control_dir, node_id), text)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_configs_round_trip() {
        let spec = ClusterSpec {
            node_count: 3,
            replication_factor: 2,
            ..ClusterSpec::default()
        };
        let dir = std::env::temp_dir().join(format!("cachex-cfg-test-{}", std::process::id()));
        write_all_configs(&dir, &spec).unwrap();

        for node_id in 1..=3u16 {
            let text = std::fs::read_to_string(config_path(&dir, node_id)).unwrap();
            let parsed: Config = toml::from_str(&text).unwrap();
            assert_eq!(parsed.node.id, node_id as u32);
            assert_eq!(parsed.node.port, spec.public_port(node_id));
            assert_eq!(parsed.metrics.port, spec.metrics_port(node_id));
            assert_eq!(parsed.cluster.nodes.len(), 3);
            assert!(parsed.aof.enabled);
            assert_eq!(parsed.replication.factor, 2);
            assert!(parsed.replication.enabled);
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rf_one_disables_replication() {
        let spec = ClusterSpec::default();
        let config = build_config(&spec, 1);
        assert!(!config.replication.enabled);
        assert_eq!(config.replication.factor, 1);
    }

    #[test]
    fn ports_derive_from_spec() {
        let spec = ClusterSpec::default();
        assert_eq!(spec.public_port(1), 7001);
        assert_eq!(spec.public_port(3), 7003);
        assert_eq!(spec.internal_port(2), 8002);
        assert_eq!(spec.metrics_port(2), 9002);
        assert_eq!(spec.public_address(2), "127.0.0.1:7002");
    }
}