

use std::sync::atomic::{AtomicU64, Ordering};

const BUCKET_BOUNDS_US: [u64; 19] = [
    1, 2, 5, 10, 20, 50, 100, 200, 500, 1_000, 2_000, 5_000, 10_000, 20_000, 50_000, 100_000,
    200_000, 500_000, 1_000_000,
];

pub struct Histogram {
    buckets: Vec<AtomicU64>,
    count: AtomicU64,
    sum_us: AtomicU64,
    max_us: AtomicU64,
}

impl Default for Histogram {
    fn default() -> Self {
        Self::new()
    }
}

impl Histogram {
    pub fn new() -> Self {
        Histogram {
            buckets: (0..=BUCKET_BOUNDS_US.len())
                .map(|_| AtomicU64::new(0))
                .collect(),
            count: AtomicU64::new(0),
            sum_us: AtomicU64::new(0),
            max_us: AtomicU64::new(0),
        }
    }

    
    pub fn record_us(&self, us: u64) {
        let index = match BUCKET_BOUNDS_US.binary_search(&us) {
            Ok(i) => i,
            Err(i) => i.min(BUCKET_BOUNDS_US.len()),
        };
        self.buckets[index].fetch_add(1, Ordering::Relaxed);
        self.count.fetch_add(1, Ordering::Relaxed);
        self.sum_us.fetch_add(us, Ordering::Relaxed);
        self.max_us.fetch_max(us, Ordering::Relaxed);
    }

    pub fn count(&self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }

    pub fn sum_us(&self) -> u64 {
        self.sum_us.load(Ordering::Relaxed)
    }

    
    pub fn max_us(&self) -> u64 {
        self.max_us.load(Ordering::Relaxed)
    }

    
    pub fn avg_us(&self) -> u64 {
        let n = self.count.load(Ordering::Relaxed);
        if n == 0 {
            0
        } else {
            self.sum_us.load(Ordering::Relaxed) / n
        }
    }

    
    
    
    pub fn percentile_us(&self, p: f64) -> u64 {
        let n = self.count.load(Ordering::Relaxed);
        if n == 0 {
            return 0;
        }
        let target = ((n as f64) * (p / 100.0)).ceil().max(1.0) as u64;
        let mut cumulative = 0u64;
        for (i, bucket) in self.buckets.iter().enumerate() {
            cumulative += bucket.load(Ordering::Relaxed);
            if cumulative >= target {
                if i == BUCKET_BOUNDS_US.len() {
                    return self.max_us.load(Ordering::Relaxed);
                }
                return BUCKET_BOUNDS_US[i];
            }
        }
        BUCKET_BOUNDS_US[BUCKET_BOUNDS_US.len() - 1]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn empty_histogram_reads_zero() {
        let h = Histogram::new();
        assert_eq!(h.count(), 0);
        assert_eq!(h.avg_us(), 0);
        assert_eq!(h.max_us(), 0);
        assert_eq!(h.percentile_us(50.0), 0);
        assert_eq!(h.percentile_us(99.0), 0);
    }

    #[test]
    fn records_land_in_the_right_bucket() {
        let h = Histogram::new();
        h.record_us(0);
        h.record_us(1);
        h.record_us(7);
        h.record_us(1_500);
        h.record_us(100_000_000);

        assert_eq!(h.count(), 5);
        assert_eq!(h.max_us(), 100_000_000);
        
        
        let expected: Vec<u64> = h
            .buckets
            .iter()
            .map(|b| b.load(Ordering::Relaxed))
            .collect();
        assert_eq!(expected[0], 2);
        assert_eq!(expected[3], 1); 
        assert_eq!(expected[10], 1); 
        assert_eq!(expected[19], 1); 
    }

    #[test]
    fn percentile_uses_bucket_upper_bound() {
        let h = Histogram::new();
        
        h.record_us(1);
        h.record_us(10);
        h.record_us(100);

        
        assert_eq!(h.percentile_us(50.0), 10);
        
        assert_eq!(h.percentile_us(100.0), 100);
    }

    #[test]
    fn top_bucket_percentile_uses_max() {
        let h = Histogram::new();
        h.record_us(1);
        h.record_us(2_000_000);
        assert_eq!(h.percentile_us(100.0), 2_000_000);
    }

    #[test]
    fn avg_and_sum_are_exact() {
        let h = Histogram::new();
        h.record_us(10);
        h.record_us(30);
        assert_eq!(h.sum_us(), 40);
        assert_eq!(h.avg_us(), 20);
    }

    #[test]
    fn concurrent_recording_is_lossless() {
        let h = Arc::new(Histogram::new());
        let mut handles = Vec::new();
        for _ in 0..8 {
            let h = h.clone();
            handles.push(std::thread::spawn(move || {
                for i in 0..10_000u64 {
                    h.record_us(i % 1_000);
                }
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }
        assert_eq!(h.count(), 80_000);
        assert_eq!(h.sum_us(), (0..10_000u64).map(|i| i % 1_000).sum::<u64>() * 8);
    }
}