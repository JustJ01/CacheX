

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub nodes: usize,
    pub router: String,
    pub vnodes: usize,
    pub clients: usize,
    pub requests: u64,
    pub keys: u64,
    pub value_size: usize,
    pub get_ratio: f64,
    pub seed: u64,
    pub key_order: String,
    pub total_secs: f64,
    pub ops_per_sec: f64,
    pub gets: u64,
    pub sets: u64,
    pub hits: u64,
    pub misses: u64,
    pub errors: u64,
    pub avg_us: u64,
    pub p50_us: u64,
    pub p95_us: u64,
    pub p99_us: u64,
    pub max_us: u64,
}

impl Report {
    pub fn header() -> &'static str {
        "nodes,router,vnodes,clients,requests,keys,value_size,get_ratio,seed,key_order,\
         total_secs,ops_per_sec,gets,sets,hits,misses,errors,avg_us,p50_us,p95_us,p99_us,max_us"
    }

    pub fn csv_row(&self) -> String {
        format!(
            "{},{},{},{},{},{},{},{:.6},{},{},{:.4},{:.2},{},{},{},{},{},{},{},{},{},{}",
            self.nodes,
            self.router,
            self.vnodes,
            self.clients,
            self.requests,
            self.keys,
            self.value_size,
            self.get_ratio,
            self.seed,
            self.key_order,
            self.total_secs,
            self.ops_per_sec,
            self.gets,
            self.sets,
            self.hits,
            self.misses,
            self.errors,
            self.avg_us,
            self.p50_us,
            self.p95_us,
            self.p99_us,
            self.max_us,
        )
    }

    pub fn summary(&self) -> String {
        format!(
            "benchmark complete: {clients} client(s), {requests} requests over {total:.2}s \
             ({ops:.0} ops/sec), gets={gets} sets={sets} hits={hits} misses={misses} errors={errors}\n\
             latency avg={avg}us p50={p50}us p95={p95}us p99={p99}us max={max}us",
            clients = self.clients,
            requests = self.requests,
            total = self.total_secs,
            ops = self.ops_per_sec,
            gets = self.gets,
            sets = self.sets,
            hits = self.hits,
            misses = self.misses,
            errors = self.errors,
            avg = self.avg_us,
            p50 = self.p50_us,
            p95 = self.p95_us,
            p99 = self.p99_us,
            max = self.max_us,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Report {
        Report {
            nodes: 3,
            router: "consistent".to_string(),
            vnodes: 100,
            clients: 8,
            requests: 10_000,
            keys: 1000,
            value_size: 32,
            get_ratio: 0.9,
            seed: 7,
            key_order: "sequential".to_string(),
            total_secs: 1.5,
            ops_per_sec: 6666.0,
            gets: 9000,
            sets: 1000,
            hits: 7000,
            misses: 2000,
            errors: 0,
            avg_us: 50,
            p50_us: 40,
            p95_us: 120,
            p99_us: 250,
            max_us: 1000,
        }
    }

    #[test]
    fn csv_header_and_row_have_matching_fields() {
        let row = sample().csv_row();
        assert_eq!(Report::header().split(',').count(), row.split(',').count());
    }

    #[test]
    fn csv_row_round_trips_parameters() {
        let row = sample().csv_row();
        let fields: Vec<&str> = row.split(',').collect();
        assert_eq!(fields[0], "3");
        assert_eq!(fields[4], "10000");
        assert_eq!(fields[8], "7");
        assert_eq!(fields[9], "sequential"); 
        assert_eq!(fields[12], "9000"); 
        assert_eq!(fields[20], "250"); 
    }

    #[test]
    fn summary_mentions_throughput() {
        let summary = sample().summary();
        assert!(summary.contains("ops/sec"), "summary missing throughput: {summary}");
        assert!(summary.contains("p99"), "summary missing p99: {summary}");
    }
}