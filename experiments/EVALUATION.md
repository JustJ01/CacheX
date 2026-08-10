# CacheX M8 — Final Evaluation Report

This report consolidates the CacheX milestone evaluation (M1–M7), the M7
experiment results, and the dashboard scenario validations into final
conclusions. All numbers quoted here are taken from `experiments\results\*.csv`
and `scripts\scenarios\results\*.csv`; every claim below was re-verified
against those files in the M8 pass.

---

## 1. Implementation status (M1–M6)

All four workspace crates are implemented and tested. 30 integration tests
pass across seven test files.

| Milestone | Deliverable | Evidence |
|-----------|-------------|----------|
| M1 | Core types + protocol + single-node cache | `cachex-core`, `cachex-server`; `smoke.rs` (7 tests) |
| M2 | TTL + LRU eviction | `storage.rs` (intrusive LRU, O(1) recency) |
| M3 | Partitioning + client routing | `hashing.rs`, `cachex-client`; `routing.rs` (5), `pool.rs` (2) |
| M4 | Async replication + failure detection | `replication.rs`, `heartbeat.rs`; `replication.rs` (5), `stabilization.rs` (6) |
| M5 | AOF persistence + metrics API | `aof.rs`, `metrics_http.rs`; `aof.rs` (3), `metrics_api.rs` (2) |
| M6 | Workload benchmark | `cachex-bench` |

Implementation is complete.

---

## 2. Experiment results (M7) — final findings

Each experiment was run to completion and its recorded claim was checked
against the raw CSV. All six check out.

### 2.1 LRU eviction
1 MiB cache, 128-byte values → effective capacity 5,140 keys. Hit ratio fell
from 73.4% at 7,000 keys to 25.7% at 20,000 keys; evictions tracked exactly
(the resident set stayed at 5,140 and evictions reached 14,860 at 20k).

### 2.2 TTL expiry
5,000 keys with a 10 s TTL. Hit rate held at 100% through 7.7 s and dropped
to 0% at 10.1 s — expiry at the configured boundary.

### 2.3 Hashing (3 → 4 nodes)
Modulo moved 74.9% of keys; consistent hashing moved 26.0% (100k keys,
vnodes=100). Both stayed load-balanced (modulo → 25/25/25/25%, consistent →
26/21/27/26%).

### 2.4 Replication (RF=2 vs RF=1)
Same 200k-request workload: RF=2 cost roughly half the throughput (12.3k vs
23.7k ops/s) and doubled memory (11.3 MB vs 5.7 MB; keys 55,538 vs 27,769 =
exactly 2 copies). Replication was fully caught up (40,315 sent = 40,315
received, gap 0) — the cost is write amplification, not lost writes.

### 2.5 Scalability
- Nodes 1→5 at fixed 8 clients: throughput roughly flat (16.5k→22.9k ops/s) →
  the cluster is client-limited at this concurrency.
- Clients 10→1000 at fixed 3 nodes: throughput held ~19–25k ops/s while average
  latency grew 0.5 ms→27.7 ms and P99 2 ms→100 ms → servers saturate and queue.

### 2.6 Failure detection & recovery
Killing node 2 of 3 (heartbeat 1 s / timeout 500 ms / miss_threshold 2):
detected Failed in ~2.0 s; ~35% of requests failed during the outage (equal to
the dead node's routing share; no automatic failover by design). Restart to
serving ~0.55 s, AOF replay ~0.33 s, Healthy again ~1.65 s. Final scan: zero
errors, data intact.

---

## 3. Dashboard scenario validation (M7)

Eight dashboard scenarios were run end-to-end against the live 3-node cluster.
All arithmetic invariants held in every CSV (requests = gets+sets, gets =
hits+misses, errors = 0, get-ratio honored).

| # | Scenario | Observed | Verdict |
|---|----------|----------|---------|
| 1 | Load (read-heavy) | 16.1k ops/s, 69.6% hit | Correct |
| 2 | Write-heavy | 19.7k ops/s, 92.5% hit | Correct |
| 3 | Hit rate → 100% | 100% (297,013 hits / 0 misses) | Correct after fix |
| 4 | Eviction | memory pinned at 100%, 4,220 evictions, hit 75.2% | Correct (live-verified) |
| 5 | TTL | 84.3% → 1.0% after 10 s expiry | Correct |
| 6 | Failure | warm 48.2% hit; node down/up | Correct (dashboard-observed) |
| 7 | Replication RF=2 | 10.3k ops/s (~half of RF=1), hit 75.2% | Correct |
| 8 | Scalability | 13.9k→21.9k→23.9k ops/s, p99 1→5→20 ms | Correct |

**Scenario 3 defect found and fixed.** The warm-up phase used sequential key
order with 8 clients; because every bench worker restarts its key sequence at
0, only keys 0..12,499 of the 20,000-key keyspace were ever written, capping
the read-phase hit rate at ~65%. Fixed by raising warm-up requests from
100,000 to 200,000 (so every client cycles the full keyspace). Re-run result:
297,013 hits / 0 misses = 100% hit rate.

---

## 4. Conclusions

1. **The cache is correct.** Eviction, TTL, hashing, replication, and
   failure-recovery all behave as designed and match their recorded numbers.
2. **The cost of correctness is measurable.** RF=2 halves write throughput
   (write amplification), and client concurrency saturates the server rather
   than scaling throughput — both are expected, documented trade-offs.
3. **Bench tooling was the source of the only observed discrepancy.** The
   scenario-3 hit-rate shortfall was a workload-coverage bug in the bench's
   per-worker key sequencing, not a cache bug. The fix is in place and
   verified.
4. **Experiments are reproducible.** All scripts use fixed seeds; re-running
   them reproduces the recorded structural numbers.

M7 and M8 are complete. All findings in `experiments\README.md` are now final.
