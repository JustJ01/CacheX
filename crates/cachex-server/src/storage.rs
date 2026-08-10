

use cachex_core::entry::Entry;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

const ENTRY_OVERHEAD_BYTES: u64 = 64;

struct IndexedEntry {
    entry: Entry,
    lru_index: usize,
    bytes: u64,
}

struct LruNode {
    key: String,
    prev: Option<usize>,
    next: Option<usize>,
}

struct LruList {
    nodes: Vec<LruNode>,
    head: Option<usize>,
    tail: Option<usize>,
    free: Vec<usize>,
}

impl LruList {
    fn new() -> Self {
        LruList {
            nodes: Vec::new(),
            head: None,
            tail: None,
            free: Vec::new(),
        }
    }

    fn alloc(&mut self, key: String) -> usize {
        match self.free.pop() {
            Some(index) => {
                self.nodes[index] = LruNode {
                    key,
                    prev: None,
                    next: None,
                };
                index
            }
            None => {
                self.nodes.push(LruNode {
                    key,
                    prev: None,
                    next: None,
                });
                self.nodes.len() - 1
            }
        }
    }

    fn push_mru(&mut self, index: usize) {
        let (prev, _) = {
            let node = &mut self.nodes[index];
            node.prev = self.tail;
            node.next = None;
            (node.prev, node.next)
        };
        match prev {
            Some(tail) => self.nodes[tail].next = Some(index),
            None => self.head = Some(index),
        }
        self.tail = Some(index);
    }

    fn unlink(&mut self, index: usize) {
        let (prev, next) = {
            let node = &self.nodes[index];
            (node.prev, node.next)
        };
        match prev {
            Some(p) => self.nodes[p].next = next,
            None => self.head = next,
        }
        match next {
            Some(n) => self.nodes[n].prev = prev,
            None => self.tail = prev,
        }
        let node = &mut self.nodes[index];
        node.prev = None;
        node.next = None;
    }

    fn touch(&mut self, index: usize) {
        if self.tail == Some(index) {
            return;
        }
        self.unlink(index);
        self.push_mru(index);
    }

    
    fn pop_lru(&mut self) -> Option<(usize, String)> {
        let index = self.head?;
        let key = std::mem::take(&mut self.nodes[index].key);
        self.unlink(index);
        self.free.push(index);
        Some((index, key))
    }
}

struct CacheInner {
    map: HashMap<String, IndexedEntry>,
    lru: LruList,
    used_bytes: u64,
    max_memory_bytes: u64,
}

impl CacheInner {
    fn entry_size(key: &str, data_len: usize) -> u64 {
        key.len() as u64 + data_len as u64 + ENTRY_OVERHEAD_BYTES
    }

    
    
    fn get(&mut self, key: &str) -> (Option<Vec<u8>>, bool) {
        let remove_index = {
            let ie = match self.map.get_mut(key) {
                Some(ie) => ie,
                None => return (None, false),
            };
            if !ie.entry.is_expired() {
                ie.entry.touch();
                self.lru.touch(ie.lru_index);
                return (Some(ie.entry.data.clone()), false);
            }
            let index = ie.lru_index;
            self.used_bytes = self.used_bytes.saturating_sub(ie.bytes);
            index
        };
        self.map.remove(key);
        self.lru.unlink(remove_index);
        self.lru.free.push(remove_index);
        (None, true)
    }

    
    
    fn set(&mut self, key: &str, value: Vec<u8>, ttl: Option<u64>) -> u64 {
        let bytes = Self::entry_size(key, value.len());
        if let Some(ie) = self.map.get_mut(key) {
            self.used_bytes = self.used_bytes.saturating_sub(ie.bytes) + bytes;
            ie.entry = Entry::new(value, ttl);
            ie.bytes = bytes;
            self.lru.touch(ie.lru_index);
        } else {
            let index = self.lru.alloc(key.to_string());
            self.lru.push_mru(index);
            self.used_bytes += bytes;
            self.map.insert(
                key.to_string(),
                IndexedEntry {
                    entry: Entry::new(value, ttl),
                    lru_index: index,
                    bytes,
                },
            );
        }
        self.evict_if_needed()
    }

    fn delete(&mut self, key: &str) -> bool {
        if let Some(ie) = self.map.remove(key) {
            self.lru.unlink(ie.lru_index);
            self.lru.free.push(ie.lru_index);
            self.used_bytes = self.used_bytes.saturating_sub(ie.bytes);
            true
        } else {
            false
        }
    }

    fn purge_expired(&mut self) -> usize {
        let expired: Vec<String> = self
            .map
            .iter()
            .filter(|(_, ie)| ie.entry.is_expired())
            .map(|(key, _)| key.clone())
            .collect();
        let mut count = 0;
        for key in expired {
            if self.delete(&key) {
                count += 1;
            }
        }
        count
    }

    fn evict_if_needed(&mut self) -> u64 {
        let mut evicted = 0;
        while self.used_bytes > self.max_memory_bytes {
            let Some((_, key)) = self.lru.pop_lru() else { break };
            if let Some(ie) = self.map.remove(&key) {
                self.used_bytes = self.used_bytes.saturating_sub(ie.bytes);
                evicted += 1;
            }
        }
        evicted
    }
}

pub struct CacheStore {
    inner: Mutex<CacheInner>,
    evictions: AtomicU64,
    ttl_expirations: AtomicU64,
}

impl CacheStore {
    pub fn new(max_memory_bytes: u64) -> Self {
        CacheStore {
            inner: Mutex::new(CacheInner {
                map: HashMap::new(),
                lru: LruList::new(),
                used_bytes: 0,
                max_memory_bytes,
            }),
            evictions: AtomicU64::new(0),
            ttl_expirations: AtomicU64::new(0),
        }
    }

    
    
    pub fn get(&self, key: &str) -> (Option<Vec<u8>>, bool) {
        let mut inner = self.inner.lock().expect("storage lock poisoned");
        let (value, expired) = inner.get(key);
        if expired {
            self.ttl_expirations.fetch_add(1, Ordering::Relaxed);
        }
        (value, expired)
    }

    pub fn set(&self, key: &str, value: Vec<u8>, ttl: Option<u64>) {
        let mut inner = self.inner.lock().expect("storage lock poisoned");
        let evicted = inner.set(key, value, ttl);
        self.evictions.fetch_add(evicted, Ordering::Relaxed);
    }

    pub fn delete(&self, key: &str) -> bool {
        self.inner
            .lock()
            .expect("storage lock poisoned")
            .delete(key)
    }

    
    pub fn purge_expired(&self) -> usize {
        let mut inner = self.inner.lock().expect("storage lock poisoned");
        let removed = inner.purge_expired();
        self.ttl_expirations.fetch_add(removed as u64, Ordering::Relaxed);
        removed
    }

    
    
    pub fn snapshot(&self) -> Vec<(String, Vec<u8>, Option<u64>)> {
        let inner = self.inner.lock().expect("storage lock poisoned");
        let mut entries = Vec::with_capacity(inner.map.len());
        for (key, ie) in inner.map.iter() {
            let ttl = ie
                .entry
                .ttl_remaining()
                .map(|d| d.as_secs().max(0));
            entries.push((key.clone(), ie.entry.data.clone(), ttl));
        }
        entries
    }

    pub fn key_count(&self) -> usize {
        self.inner
            .lock()
            .expect("storage lock poisoned")
            .map
            .len()
    }

    pub fn memory_usage(&self) -> u64 {
        self.inner.lock().expect("storage lock poisoned").used_bytes
    }

    pub fn max_memory(&self) -> u64 {
        self.inner.lock().expect("storage lock poisoned").max_memory_bytes
    }

    
    pub fn stats(&self) -> (usize, u64, u64) {
        let inner = self.inner.lock().expect("storage lock poisoned");
        (inner.map.len(), inner.used_bytes, inner.max_memory_bytes)
    }

    pub fn eviction_count(&self) -> u64 {
        self.evictions.load(Ordering::Relaxed)
    }

    pub fn ttl_expiration_count(&self) -> u64 {
        self.ttl_expirations.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VAL: &[u8] = b"0123456789";

    #[test]
    fn set_get_delete() {
        let store = CacheStore::new(1_000_000);
        store.set("a", VAL.to_vec(), None);
        assert_eq!(store.get("a").0, Some(VAL.to_vec()));
        assert_eq!(store.get("missing").0, None);
        assert!(store.delete("a"));
        assert!(!store.delete("a"));
        assert_eq!(store.get("a").0, None);
        assert_eq!(store.key_count(), 0);
    }

    #[test]
    fn overwrite_replaces_value() {
        let store = CacheStore::new(1_000_000);
        store.set("a", b"one".to_vec(), None);
        store.set("a", b"two".to_vec(), None);
        assert_eq!(store.get("a").0, Some(b"two".to_vec()));
        assert_eq!(store.key_count(), 1);
    }

    #[test]
    fn lru_evicts_least_recently_used() {
        
        let store = CacheStore::new(150);
        store.set("a", VAL.to_vec(), None);
        store.set("b", VAL.to_vec(), None);
        store.set("c", VAL.to_vec(), None);
        assert_eq!(store.get("a").0, None, "a is LRU and must be evicted");
        assert_eq!(store.get("b").0, Some(VAL.to_vec()));
        assert_eq!(store.get("c").0, Some(VAL.to_vec()));
        assert_eq!(store.eviction_count(), 1);
        assert_eq!(store.key_count(), 2);
    }

    #[test]
    fn touch_prevents_eviction() {
        let store = CacheStore::new(150);
        store.set("a", VAL.to_vec(), None);
        store.set("b", VAL.to_vec(), None);
        store.get("a"); 
        store.set("c", VAL.to_vec(), None);
        assert_eq!(store.get("b").0, None, "b is LRU after a was touched");
        assert_eq!(store.get("a").0, Some(VAL.to_vec()));
        assert_eq!(store.get("c").0, Some(VAL.to_vec()));
    }

    #[test]
    fn delete_keeps_lru_order_consistent() {
        let store = CacheStore::new(150);
        store.set("a", VAL.to_vec(), None); 
        store.set("b", VAL.to_vec(), None); 
        store.set("c", VAL.to_vec(), None); 
        store.delete("b");                  
        store.set("d", VAL.to_vec(), None); 
        assert_eq!(store.get("a").0, None, "a was already evicted");
        assert_eq!(store.get("b").0, None, "b was deleted");
        assert_eq!(store.get("c").0, Some(VAL.to_vec()), "c must survive");
        assert_eq!(store.get("d").0, Some(VAL.to_vec()));
        assert_eq!(store.eviction_count(), 1);
        assert_eq!(store.key_count(), 2);
    }

    #[test]
    fn ttl_expires_lazily_on_get() {
        let store = CacheStore::new(1_000_000);
        store.set("otp", b"1234".to_vec(), Some(0));
        let (value, expired) = store.get("otp");
        assert!(value.is_none());
        assert!(expired);
        assert_eq!(store.ttl_expiration_count(), 1);
        assert_eq!(store.key_count(), 0);
    }

    #[test]
    fn ttl_still_valid_within_window() {
        let store = CacheStore::new(1_000_000);
        store.set("k", b"v".to_vec(), Some(60));
        assert_eq!(store.get("k").0, Some(b"v".to_vec()));
        assert_eq!(store.ttl_expiration_count(), 0);
    }

    #[test]
    fn purge_expired_removes_expired_only() {
        let store = CacheStore::new(1_000_000);
        store.set("x", b"1".to_vec(), Some(0));
        store.set("y", b"2".to_vec(), None);
        assert_eq!(store.purge_expired(), 1);
        assert_eq!(store.key_count(), 1);
        assert_eq!(store.get("x").0, None);
        assert_eq!(store.get("y").0, Some(b"2".to_vec()));
    }

    #[test]
    fn memory_usage_accounted() {
        let store = CacheStore::new(1_000_000);
        store.set("a", VAL.to_vec(), None);
        assert_eq!(store.memory_usage(), CacheInner::entry_size("a", VAL.len()));
        store.delete("a");
        assert_eq!(store.memory_usage(), 0);
    }
}