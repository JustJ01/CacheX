

use std::collections::{BTreeMap, HashSet};

pub const DEFAULT_VIRTUAL_NODES: usize = 100;

pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &byte in bytes {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x1_0000_0001_b3);
    }
    hash = (hash ^ (hash >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    hash = (hash ^ (hash >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    hash ^ (hash >> 31)
}

pub trait Router {
    
    fn primary(&self, key: &str) -> &str;

    
    
    fn replicas(&self, key: &str, count: usize) -> Vec<&str>;

    
    fn nodes(&self) -> &[String];
}

pub fn moved_fraction<A: Router, B: Router>(before: &A, after: &B, keys: &[String]) -> f64 {
    let mut moved = 0usize;
    for key in keys {
        if before.primary(key) != after.primary(key) {
            moved += 1;
        }
    }
    moved as f64 / keys.len().max(1) as f64
}

pub struct ConsistentHasher {
    
    ring: BTreeMap<u64, usize>,
    nodes: Vec<String>,
    vnodes: usize,
}

impl ConsistentHasher {
    
    
    pub fn new(mut nodes: Vec<String>, vnodes: usize) -> Self {
        assert!(!nodes.is_empty(), "consistent hasher needs at least one node");
        assert!(vnodes > 0, "vnodes must be positive");
        nodes.sort();
        nodes.dedup();

        let mut ring = BTreeMap::new();
        for (index, node) in nodes.iter().enumerate() {
            for vnode in 0..vnodes {
                let position = fnv1a64(format!("{node}#{vnode}").as_bytes());
                ring.entry(position).or_insert(index);
            }
        }

        ConsistentHasher { ring, nodes, vnodes }
    }

    pub fn virtual_nodes(&self) -> usize {
        self.vnodes
    }

    
    
    pub fn with_nodes(&self, nodes: Vec<String>) -> Self {
        ConsistentHasher::new(nodes, self.vnodes)
    }
}

impl Router for ConsistentHasher {
    fn primary(&self, key: &str) -> &str {
        let hash = fnv1a64(key.as_bytes());
        let index = self
            .ring
            .range(hash..)
            .next()
            .or_else(|| self.ring.first_key_value())
            .map(|(_, index)| *index)
            .expect("ring is never empty");
        &self.nodes[index]
    }

    fn replicas(&self, key: &str, count: usize) -> Vec<&str> {
        if count == 0 {
            return Vec::new();
        }
        let hash = fnv1a64(key.as_bytes());
        let primary_index = self
            .ring
            .range(hash..)
            .next()
            .or_else(|| self.ring.first_key_value())
            .map(|(_, index)| *index)
            .expect("ring is never empty");

        let mut seen = HashSet::new();
        seen.insert(primary_index);
        let mut replicas = Vec::new();
        for (_, index) in self.ring.range(hash..).chain(self.ring.range(..hash)) {
            if seen.insert(*index) {
                replicas.push(self.nodes[*index].as_str());
                if replicas.len() == count {
                    break;
                }
            }
        }
        replicas
    }

    fn nodes(&self) -> &[String] {
        &self.nodes
    }
}

pub struct ModuloHasher {
    nodes: Vec<String>,
}

impl ModuloHasher {
    pub fn new(mut nodes: Vec<String>) -> Self {
        assert!(!nodes.is_empty(), "modulo hasher needs at least one node");
        nodes.sort();
        nodes.dedup();
        ModuloHasher { nodes }
    }
}

impl Router for ModuloHasher {
    fn primary(&self, key: &str) -> &str {
        let hash = fnv1a64(key.as_bytes());
        &self.nodes[hash as usize % self.nodes.len()]
    }

    fn replicas(&self, key: &str, count: usize) -> Vec<&str> {
        let hash = fnv1a64(key.as_bytes());
        let start = hash as usize % self.nodes.len();
        (0..count)
            .map(|offset| self.nodes[(start + 1 + offset) % self.nodes.len()].as_str())
            .collect()
    }

    fn nodes(&self) -> &[String] {
        &self.nodes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(count: usize) -> Vec<String> {
        (0..count).map(|i| format!("key:{i}")).collect()
    }

    #[test]
    fn fnv1a_is_deterministic() {
        let a = fnv1a64(b"user:101");
        let b = fnv1a64(b"user:101");
        assert_eq!(a, b);
        assert_ne!(a, fnv1a64(b"user:102"));
    }

    #[test]
    fn fnv_uniformity_smoke() {
        let mut buckets = [0u64; 16];
        for i in 0..20000u32 {
            let h = fnv1a64(format!("key:{i}").as_bytes());
            buckets[(h >> 60) as usize] += 1;
        }
        let min = *buckets.iter().min().unwrap();
        let max = *buckets.iter().max().unwrap();
        eprintln!("fnv buckets: {buckets:?}");
        assert!(max < 2 * min, "fnv not uniform: min {min} max {max}");
    }

    #[test]
    fn consistent_primary_is_stable() {
        let ring = ConsistentHasher::new(vec!["a".into(), "b".into(), "c".into()], 100);
        let first = ring.primary("user:101").to_string();
        for _ in 0..5 {
            assert_eq!(ring.primary("user:101"), first);
        }
    }

    #[test]
    fn consistent_primary_lives_in_node_set() {
        let nodes = vec!["127.0.0.1:7001".into(), "127.0.0.1:7002".into(), "127.0.0.1:7003".into()];
        let ring = ConsistentHasher::new(nodes.clone(), 100);
        for key in keys(10_000) {
            assert!(nodes.contains(&ring.primary(&key).to_string()));
        }
    }

    #[test]
    fn consistent_load_is_reasonably_balanced() {
        let ring = ConsistentHasher::new(vec!["a".into(), "b".into(), "c".into()], 100);
        let mut counts = std::collections::HashMap::<String, usize>::new();
        for key in keys(20_000) {
            *counts.entry(ring.primary(&key).to_string()).or_default() += 1;
        }
        assert_eq!(counts.len(), 3);
        let max = *counts.values().max().unwrap();
        let min = *counts.values().min().unwrap();
        assert!(max < 2 * min, "load imbalance too high: {counts:?}");
    }

    #[test]
    fn consistent_moves_far_fewer_keys_than_modulo_on_node_add() {
        let three = vec!["a".into(), "b".into(), "c".into()];
        let four = vec!["a".into(), "b".into(), "c".into(), "d".into()];
        let keys = keys(100_000);

        let ring_3 = ConsistentHasher::new(three.clone(), 100);
        let ring_4 = ConsistentHasher::new(four.clone(), 100);
        let consistent_moved = moved_fraction(&ring_3, &ring_4, &keys);

        let modulo_3 = ModuloHasher::new(three.clone());
        let modulo_4 = ModuloHasher::new(four.clone());
        let modulo_moved = moved_fraction(&modulo_3, &modulo_4, &keys);

        assert!(
            consistent_moved < 0.40,
            "consistent hashing should move ~25% of keys, got {consistent_moved}"
        );
        assert!(
            modulo_moved > 0.60,
            "modulo hashing should move ~75% of keys, got {modulo_moved}"
        );
        assert!(
            consistent_moved < modulo_moved / 2.0,
            "consistent ({consistent_moved}) must move far fewer keys than modulo ({modulo_moved})"
        );
    }

    #[test]
    fn consistent_replicas_are_distinct_and_exclude_primary() {
        let nodes = vec!["a".into(), "b".into(), "c".into(), "d".into()];
        let ring = ConsistentHasher::new(nodes.clone(), 50);
        for key in keys(5_000) {
            let primary = ring.primary(&key).to_string();
            let replicas = ring.replicas(&key, 2);
            assert_eq!(replicas.len(), 2);
            assert!(replicas.iter().all(|r| *r != primary), "replica equals primary");
            assert!(replicas[0] != replicas[1], "replicas not distinct");
            assert!(replicas.iter().all(|r| nodes.contains(&r.to_string())));
        }
    }

    #[test]
    fn consistent_replicas_are_stable() {
        let ring = ConsistentHasher::new(vec!["a".into(), "b".into(), "c".into()], 100);
        let first = ring.replicas("user:101", 1);
        for _ in 0..5 {
            assert_eq!(ring.replicas("user:101", 1), first);
        }
    }

    #[test]
    fn consistent_replica_set_shrinks_for_tiny_cluster() {
        let ring = ConsistentHasher::new(vec!["a".into()], 10);
        assert!(ring.replicas("k", 2).is_empty(), "no other node exists");
        assert_eq!(ring.primary("k"), "a");
    }

    #[test]
    fn modulo_replicas_cycle_and_exclude_primary() {
        let nodes = vec!["a".into(), "b".into(), "c".into(), "d".into()];
        let modulo = ModuloHasher::new(nodes.clone());
        for key in keys(2_000) {
            let primary = modulo.primary(&key).to_string();
            let replicas = modulo.replicas(&key, 2);
            assert_eq!(replicas.len(), 2);
            assert!(replicas.iter().all(|r| *r != primary));
            assert!(replicas[0] != replicas[1]);
        }
    }

    #[test]
    fn temp_failure_key_distribution() {
        let nodes: Vec<String> = vec!["127.0.0.1:7001".into(), "127.0.0.1:7002".into(), "127.0.0.1:7003".into()];
        let ring = ConsistentHasher::new(nodes.clone(), 100);
        let mut counts = std::collections::HashMap::<String, usize>::new();
        for i in 0..20000u64 {
            let key = format!("key:{:08}", i);
            *counts.entry(ring.primary(&key).to_string()).or_default() += 1;
        }
        for n in &nodes {
            eprintln!("node {n}: {} keys", counts.get(n).copied().unwrap_or(0));
        }
    }

    #[test]
    fn vnodes_affect_balance_when_few_nodes() {
        
        
        let ring_few = ConsistentHasher::new(vec!["a".into(), "b".into()], 1);
        let ring_many = ConsistentHasher::new(vec!["a".into(), "b".into()], 200);
        let ratio = |r: &ConsistentHasher| {
            let mut counts = std::collections::HashMap::<String, usize>::new();
            for key in keys(50_000) {
                *counts.entry(r.primary(&key).to_string()).or_default() += 1;
            }
            *counts.values().max().unwrap() as f64 / *counts.values().min().unwrap() as f64
        };
        assert!(ratio(&ring_many) <= ratio(&ring_few), "more vnodes should not hurt balance");
    }
}