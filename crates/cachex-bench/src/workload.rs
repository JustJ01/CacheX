

pub struct SplitMix64(u64);

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        SplitMix64(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    
    pub fn below(&mut self, bound: u64) -> u64 {
        debug_assert!(bound > 0);
        let threshold = bound.wrapping_neg() % bound;
        loop {
            let value = self.next_u64();
            if value >= threshold {
                return value % bound;
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyOrder {
    
    Uniform,
    
    
    
    Sequential,
}

impl KeyOrder {
    pub fn as_str(&self) -> &'static str {
        match self {
            KeyOrder::Uniform => "uniform",
            KeyOrder::Sequential => "sequential",
        }
    }
}

impl std::fmt::Display for KeyOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

pub struct Workload {
    keyspace: u64,
    value_size: usize,
    get_ratio: f64,
    key_order: KeyOrder,
    sequence: u64,
    rng: SplitMix64,
}

impl Workload {
    pub fn new(
        keyspace: u64,
        value_size: usize,
        get_ratio: f64,
        key_order: KeyOrder,
        seed: u64,
    ) -> Self {
        Workload {
            keyspace,
            value_size,
            get_ratio,
            key_order,
            sequence: 0,
            rng: SplitMix64::new(seed),
        }
    }

    
    
    pub fn next_key(&mut self) -> String {
        match self.key_order {
            KeyOrder::Uniform => format!("key:{:08}", self.rng.below(self.keyspace)),
            KeyOrder::Sequential => {
                let key = format!("key:{:08}", self.sequence);
                self.sequence = (self.sequence + 1) % self.keyspace;
                key
            }
        }
    }

    
    
    
    
    
    
    pub fn next_value(&mut self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.value_size);
        while bytes.len() < self.value_size {
            let word = self.rng.next_u64();
            for byte in word.to_le_bytes() {
                bytes.push(b'!' + (byte % (b'~' - b'!' + 1)));
            }
        }
        bytes.truncate(self.value_size);
        bytes
    }

    
    pub fn should_get(&mut self) -> bool {
        let bound = (self.get_ratio * 1_000_000.0).clamp(0.0, 1_000_000.0) as u64;
        self.rng.below(1_000_000) < bound
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_reproduces_key_stream() {
        let mut a = Workload::new(1000, 32, 0.5, KeyOrder::Uniform, 7);
        let mut b = Workload::new(1000, 32, 0.5, KeyOrder::Uniform, 7);
        for _ in 0..1000 {
            assert_eq!(a.next_key(), b.next_key());
            assert_eq!(a.next_value(), b.next_value());
            assert_eq!(a.should_get(), b.should_get());
        }
    }

    #[test]
    fn sequential_keys_visit_every_key_exactly_once() {
        let mut workload = Workload::new(5, 16, 0.5, KeyOrder::Sequential, 1);
        let expected: Vec<String> = (0..5).map(|i| format!("key:{i:08}")).collect();
        for _ in 0..3 {
            let seen: Vec<String> = (0..5).map(|_| workload.next_key()).collect();
            assert_eq!(seen, expected, "sequential keys must cycle in order");
        }
    }

    #[test]
    fn keys_stay_within_keyspace() {
        let mut workload = Workload::new(50, 16, 0.5, KeyOrder::Uniform, 1);
        for _ in 0..10_000 {
            let key = workload.next_key();
            let number: u64 = key["key:".len()..].parse().unwrap();
            assert!(number < 50, "key {key} out of keyspace");
        }
    }

    #[test]
    fn values_have_exact_size() {
        let mut workload = Workload::new(100, 128, 0.5, KeyOrder::Uniform, 2);
        for _ in 0..100 {
            assert_eq!(workload.next_value().len(), 128);
        }
    }

    #[test]
    fn values_are_newline_safe_printable_ascii() {
        let mut workload = Workload::new(100, 512, 0.5, KeyOrder::Uniform, 2);
        for _ in 0..1000 {
            for byte in workload.next_value() {
                assert!(
                    (b'!'..=b'~').contains(&byte),
                    "value byte {byte:#04x} would break the text protocol"
                );
            }
        }
    }

    #[test]
    fn get_ratio_is_honored_statistically() {
        for (ratio, tolerance) in [(0.0, 0.0), (1.0, 0.0), (0.5, 0.03), (0.9, 0.03)] {
            let mut workload = Workload::new(1000, 16, ratio, KeyOrder::Uniform, 99);
            let mut gets = 0u64;
            let total = 100_000u64;
            for _ in 0..total {
                if workload.should_get() {
                    gets += 1;
                }
            }
            let actual = gets as f64 / total as f64;
            assert!(
                (actual - ratio).abs() <= tolerance,
                "ratio {ratio} produced {actual}"
            );
        }
    }

    #[test]
    fn splitmix_below_is_uniform() {
        let mut rng = SplitMix64::new(1234);
        let mut counts = [0u64; 10];
        for _ in 0..100_000 {
            counts[rng.below(10) as usize] += 1;
        }
        for (i, count) in counts.iter().enumerate() {
            assert!(*count > 9_000, "bucket {i} too small: {count}");
            assert!(*count < 11_000, "bucket {i} too large: {count}");
        }
    }
}