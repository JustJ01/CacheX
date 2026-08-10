

use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct Entry {
    pub data: Vec<u8>,
    
    pub expires_at: Option<Instant>,
    
    pub last_access: Instant,
    
    pub size: usize,
}

impl Entry {
    pub fn new(data: Vec<u8>, ttl: Option<u64>) -> Self {
        let now = Instant::now();
        let size = data.len();
        Entry {
            data,
            expires_at: ttl.map(|secs| now + Duration::from_secs(secs)),
            last_access: now,
            size,
        }
    }

    pub fn is_expired(&self) -> bool {
        self.expires_at.map_or(false, |expiry| Instant::now() >= expiry)
    }

    pub fn ttl_remaining(&self) -> Option<Duration> {
        self.expires_at
            .map(|expiry| expiry.saturating_duration_since(Instant::now()))
    }

    
    pub fn touch(&mut self) {
        self.last_access = Instant::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_without_ttl_never_expires() {
        let entry = Entry::new(b"Jill".to_vec(), None);
        assert!(!entry.is_expired());
        assert!(entry.ttl_remaining().is_none());
        assert_eq!(entry.size, 4);
    }

    #[test]
    fn entry_with_ttl_expires() {
        let entry = Entry::new(b"otp".to_vec(), Some(0));
        assert!(entry.is_expired());
    }

    #[test]
    fn touch_refreshes_last_access() {
        let mut entry = Entry::new(b"x".to_vec(), None);
        let before = entry.last_access;
        std::thread::sleep(Duration::from_millis(2));
        entry.touch();
        assert!(entry.last_access > before);
    }
}