

use crate::workload::KeyOrder;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouterKind {
    Consistent,
    Modulo,
}

impl RouterKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            RouterKind::Consistent => "consistent",
            RouterKind::Modulo => "modulo",
        }
    }
}

impl fmt::Display for RouterKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    
    pub nodes: Vec<String>,
    pub router: RouterKind,
    pub vnodes: usize,
    
    pub clients: usize,
    
    pub requests: u64,
    
    pub keys: u64,
    
    pub value_size: usize,
    
    pub get_ratio: f64,
    
    pub key_order: KeyOrder,
    
    pub seed: u64,
    
    pub ttl: u64,
    
    pub output: String,
}

impl Config {
    pub fn parse(args: &[String]) -> Result<Config, String> {
        let mut nodes: Vec<String> = Vec::new();
        let mut router = RouterKind::Consistent;
        let mut vnodes = 100usize;
        let mut clients = 1usize;
        let mut requests = 1_000u64;
        let mut keys = 1_000u64;
        let mut value_size = 32usize;
        let mut get_ratio = 0.5f64;
        let mut key_order = KeyOrder::Uniform;
        let mut seed = 0u64;
        let mut ttl = 0u64;
        let mut output = "bench-results.csv".to_string();

        let mut iter = args.iter();
        while let Some(flag) = iter.next() {
            let value = iter
                .next()
                .ok_or_else(|| format!("missing value for `{flag}`"))?;
            match flag.as_str() {
                "--nodes" => {
                    nodes = value
                        .split(',')
                        .map(|part| part.trim().to_string())
                        .filter(|part| !part.is_empty())
                        .collect();
                }
                "--router" => {
                    router = match value.as_str() {
                        "consistent" => RouterKind::Consistent,
                        "modulo" => RouterKind::Modulo,
                        other => {
                            return Err(format!(
                                "unknown router `{other}` (expected `consistent` or `modulo`)"
                            ))
                        }
                    };
                }
                "--vnodes" => {
                    vnodes = value
                        .parse()
                        .map_err(|_| format!("invalid --vnodes value `{value}`"))?;
                }
                "--clients" => {
                    clients = value
                        .parse()
                        .map_err(|_| format!("invalid --clients value `{value}`"))?;
                }
                "--requests" => {
                    requests = value
                        .parse()
                        .map_err(|_| format!("invalid --requests value `{value}`"))?;
                }
                "--keys" => {
                    keys = value
                        .parse()
                        .map_err(|_| format!("invalid --keys value `{value}`"))?;
                }
                "--value-size" => {
                    value_size = value
                        .parse()
                        .map_err(|_| format!("invalid --value-size value `{value}`"))?;
                }
                "--get-ratio" => {
                    get_ratio = value
                        .parse()
                        .map_err(|_| format!("invalid --get-ratio value `{value}`"))?;
                }
                "--key-order" => {
                    key_order = match value.as_str() {
                        "uniform" => KeyOrder::Uniform,
                        "sequential" => KeyOrder::Sequential,
                        other => {
                            return Err(format!(
                                "unknown key order `{other}` (expected `uniform` or `sequential`)"
                            ))
                        }
                    };
                }
                "--seed" => {
                    seed = value
                        .parse()
                        .map_err(|_| format!("invalid --seed value `{value}`"))?;
                }
                "--ttl" => {
                    ttl = value
                        .parse()
                        .map_err(|_| format!("invalid --ttl value `{value}`"))?;
                }
                "--output" => {
                    output = value.clone();
                }
                other => return Err(format!("unknown flag `{other}` (see --help)")),
            }
        }

        if nodes.is_empty() {
            return Err("`--nodes` is required: comma-separated host:port addresses".to_string());
        }
        if clients == 0 {
            return Err("`--clients` must be >= 1".to_string());
        }
        if requests == 0 {
            return Err("`--requests` must be >= 1".to_string());
        }
        if keys == 0 {
            return Err("`--keys` must be >= 1".to_string());
        }
        if value_size == 0 {
            return Err("`--value-size` must be >= 1".to_string());
        }
        if !(0.0..=1.0).contains(&get_ratio) {
            return Err("`--get-ratio` must be in [0, 1]".to_string());
        }

        Ok(Config {
            nodes,
            router,
            vnodes,
            clients,
            requests,
            keys,
            value_size,
            get_ratio,
            key_order,
            seed,
            ttl,
            output,
        })
    }
}

pub fn usage() -> &'static str {
    "CacheX workload generator (M6)

Usage:
  cachex-bench --nodes HOST:PORT[,HOST:PORT...] [options]

Required:
  --nodes LIST          cluster nodes as a comma-separated host:port list

Options:
  --router KIND         consistent | modulo   (default: consistent)
  --vnodes N            virtual ring points per node (default: 100)
  --clients N           concurrent workers    (default: 1)
  --requests N          total requests        (default: 1000)
  --keys N              keyspace size         (default: 1000)
  --value-size BYTES    SET payload bytes     (default: 32)
  --get-ratio R         fraction of GETs in [0,1] (default: 0.5)
  --key-order ORDER     uniform | sequential  (default: uniform)
  --seed N              PRNG seed             (default: 0)
  --ttl SECS            TTL for SETs, 0=none  (default: 0)
  --output FILE         CSV results file      (default: bench-results.csv)
  --help                show this help"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_full_configuration() {
        let config = Config::parse(&args(&[
            "--nodes", "127.0.0.1:7001,127.0.0.1:7002",
            "--router", "modulo",
            "--vnodes", "50",
            "--clients", "8",
            "--requests", "5000",
            "--keys", "2000",
            "--value-size", "128",
            "--get-ratio", "0.9",
            "--key-order", "sequential",
            "--seed", "42",
            "--ttl", "60",
            "--output", "out.csv",
        ]))
        .unwrap();
        assert_eq!(config.nodes.len(), 2);
        assert_eq!(config.router, RouterKind::Modulo);
        assert_eq!(config.vnodes, 50);
        assert_eq!(config.clients, 8);
        assert_eq!(config.requests, 5000);
        assert_eq!(config.keys, 2000);
        assert_eq!(config.value_size, 128);
        assert_eq!(config.get_ratio, 0.9);
        assert_eq!(config.key_order, KeyOrder::Sequential);
        assert_eq!(config.seed, 42);
        assert_eq!(config.ttl, 60);
        assert_eq!(config.output, "out.csv");
    }

    #[test]
    fn missing_nodes_is_an_error() {
        assert!(Config::parse(&[]).is_err());
        assert!(Config::parse(&args(&["--clients", "2"])).is_err());
    }

    #[test]
    fn defaults_apply() {
        let config = Config::parse(&args(&["--nodes", "127.0.0.1:7001"])).unwrap();
        assert_eq!(config.router, RouterKind::Consistent);
        assert_eq!(config.clients, 1);
        assert_eq!(config.requests, 1000);
        assert_eq!(config.get_ratio, 0.5);
        assert_eq!(config.key_order, KeyOrder::Uniform);
        assert_eq!(config.ttl, 0);
        assert_eq!(config.output, "bench-results.csv");
    }

    #[test]
    fn rejects_invalid_values() {
        assert!(Config::parse(&args(&["--nodes", "a", "--clients", "0"])).is_err());
        assert!(Config::parse(&args(&["--nodes", "a", "--get-ratio", "1.5"])).is_err());
        assert!(Config::parse(&args(&["--nodes", "a", "--router", "magic"])).is_err());
        assert!(Config::parse(&args(&["--nodes", "a", "--key-order", "hotspot"])).is_err());
        assert!(Config::parse(&args(&["--nodes", "a", "--bogus", "1"])).is_err());
    }
}