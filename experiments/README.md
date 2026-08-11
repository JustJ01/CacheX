# CacheX M7 experiments

Reproducible load experiments for the cache behaviors that lose data (LRU
eviction, TTL expiry) plus hashing, replication, scalability, and
failure-recovery. Each script drives an **isolated set of nodes** that the
script starts and stops, so nothing needs to be running beforehand. All
experiments use fixed seeds, so re-running them reproduces the same numbers
exactly.

## Setup

```powershell
cargo build -p cachex-server
cargo build -p cachex-bench
```

## Findings

All six experiments below have been run to completion, their results verified
against the recorded claims, and validated (M8 evaluation). The raw data is in
`experiments\results\*.csv`; every number quoted here matches those files.

**Finding — LRU.** Increasing the working set beyond the
effective cache capacity significantly reduced the hit ratio and increased
evictions. For the tested configuration (1 MiB cache, 128-byte values), the
hit ratio decreased from 73.4% at 7,000 keys to 25.7% at 20,000 keys.
Eviction counts tracked exactly: at 20,000 keys the resident set stayed at
5,140 keys (the measured effective capacity) and evictions reached 14,860.

**Finding — TTL.** With a configured TTL of 10 seconds, the
experiment showed complete expiration by approximately 10.1 seconds (100%
hit rate through 7.7 s, 0% at 10.1 s). The experiment was repeated using a
monotonic stopwatch after identifying and correcting a clock-measurement
issue.

**Finding — Hashing.** Growing a 3-node cluster to 4 nodes moved
74.9% of keys under modulo hashing but only 26.0% under consistent hashing
(100,000 keys, fixed seed, vnodes=100). Both stayed load-balanced: modulo
~33/33/34% became 25/25/25/25%, consistent 36/32/32% became 26/21/27/26%.

**Finding — Replication.** On an identical 3-node workload
(200k requests, 50k keys, 8 clients, 80% GET), RF=2 versus RF=1 cost roughly
half the throughput (12.3k vs 23.7k ops/s) and doubled memory (11.3 MB vs
5.7 MB; key count 55,538 vs 27,769, i.e. exactly 2 copies). Average latency
rose from 332 us to 641 us. Replication was fully caught up (40,315 sent =
40,315 received, gap 0), so the cost is the write amplification of async
replication, not lost or lagged writes.

**Finding — Scalability.** With a fixed 8 clients, throughput
was roughly flat from 1 to 5 nodes (16.5k to 22.9k ops/s), showing the
cluster is client-limited, not node-limited, at this concurrency. With a
fixed 3-node cluster, growing clients from 10 to 1,000 kept throughput
around 19-25k ops/s while average latency grew from 0.5 ms to 27.7 ms and
P99 from 2 ms to 100 ms, i.e. the servers saturate and queue.

**Finding — Failure detection & recovery.** Killing one node of
a 3-node cluster (heartbeat 1s, timeout 500ms, miss_threshold 2) was
detected as Failed in ~2.0 s. During the outage ~35% of requests failed,
which equals the dead node's routing share (CacheX deliberately has no
automatic failover). Restarting the node returned it to serving in ~0.55 s,
its AOF replayed its keys in ~0.33 s, and peers marked it Healthy again in
~1.65 s. Afterwards a full scan reported zero errors with data intact.

Raw results: `experiments\results\*.csv`

## Experiment 1 — LRU eviction: hit rate vs working set

`experiments\run-eviction.ps1`

The cache holds `capacity = max_memory_bytes / (64 + value_size)` entries
(64 bytes of per-entry overhead). A small cache is loaded with a working set
of `keyspace` distinct keys (sequential key order, written exactly once),
forcing LRU to evict everything beyond capacity. A second sequential scan
reads every key once, so:

```
hit_rate = capacity / keyspace        (for keyspace > capacity)
```

Default parameters: 1 MiB cache, 128-byte values (capacity ~5461 keys),
keyspaces 1k–20k. Eviction counters come from the node's `/metrics` endpoint.

Results: `experiments\results\eviction.csv`

```
powershell -ExecutionPolicy Bypass -File experiments\run-eviction.ps1
# tweak: -CacheMB 4 -ValueSize 256 -Out custom.csv
```

## Experiment 2 — TTL expiry: hit-rate decay

`experiments\run-ttl.ps1`

Loads `keys` keys with a `ttl` second TTL, then scans them all with GETs every
`interval` seconds. Each key stops being served once its TTL passes, so the
measured hit rate decays from ~100% toward ~0 around the TTL boundary.

Default parameters: 5000 keys, 10 s TTL, 2 s measurement interval.

Results: `experiments\results\ttl.csv`

```
powershell -ExecutionPolicy Bypass -File experiments\run-ttl.ps1
# tweak: -TTL 30 -IntervalSecs 5
```

## Experiment 3 — Hashing: keys moved when growing 3 nodes to 4

`experiments\run-hashing.ps1`

For each router (modulo, consistent) a 3-node cluster is loaded with `keys`
keys (sequential, fixed seed), and per-node request counters from `/metrics`
give the load distribution. An empty 4th node is started and every key is
re-scanned with a 4-node client ring. The miss rate of that scan *is* the
keys-moved fraction, because a key whose ring position changed is now routed
to a node that never saw it. Defaults: 100,000 keys, seed 42, vnodes 100.

Results: `experiments\results\hashing.csv`

```
powershell -ExecutionPolicy Bypass -File experiments\run-hashing.ps1
# tweak: -Keys 50000 -Seed 7
```

## Experiment 4 — Replication: RF=2 vs RF=1

`experiments\run-replication.ps1`

A 3-node cluster runs an identical workload (requests, keys, clients, value
size, GET/SET ratio, seed, consistent router) with only the replication
factor differing (1 vs 2). Replication is forwarded to the ring-successor
replica over the internal port and awaited before the client gets its ACK, so
the sent-vs-received gap after the run is the replication lag. Throughput and
latency come from the bench histogram; memory and key counts from per-node
`/metrics`. Defaults: 200k requests, 50k keys, 8 clients, 80% GET, 128-byte
values, seed 42.

Results: `experiments\results\replication.csv`

```
powershell -ExecutionPolicy Bypass -File experiments\run-replication.ps1
# tweak: -Requests 100000 -Clients 16
```

## Experiment 5 — Scalability (two independent dimensions)

`experiments\run-scalability.ps1`

Only one dimension varies per run, so the other cannot confound the result:

- Nodes 1 -> 2 -> 3 -> 4 -> 5, clients fixed at 8 (fresh cluster per count).
- Clients 10 -> 50 -> 100 -> 250 -> 500 -> 1000, nodes fixed at 3.

Defaults: 200k requests, 50k keys, 80% GET, seed 42, consistent router.

Results: `experiments\results\scalability.csv`

```
powershell -ExecutionPolicy Bypass -File experiments\run-scalability.ps1
```

## Experiment 6 — Failure detection & recovery

`experiments\run-recovery.ps1`

A healthy cluster (AOF enabled, heartbeat 1s/500ms/miss_threshold 2) loads
keys, then node 2 is force-killed. The script stopwatches each phase:
failure detection (node 1 sees peer 2 as Failed via `/metrics`), request
failure rate while the node is down (a bounded sample bench), restart to
serving, AOF replay time (log line + `recovery_ms` metric), and time to
Healthy (peers.failed back to 0). A final full scan confirms zero errors and
intact data. Defaults: 3 nodes, 100k requests, 50k keys, seed 42.

Cluster size is configurable with `-NodeCount` (3, 4, 5 supported; each size
runs on its own isolated port range). The failure share while a node is down
tracks that node's 1/N routing share, and detection time stays heartbeat-bound
(~2 s) regardless of size.

This measures detection and recovery only. CacheX deliberately has no Raft,
no leader election, and no automatic failover, so the finding is about how
fast failure is detected and how completely a node restores its own data.

Results: `experiments\results\recovery.csv` (3 nodes), `recovery-n4.csv` (4
nodes), `recovery-n5.csv` (5 nodes).

```
powershell -ExecutionPolicy Bypass -File experiments\run-recovery.ps1
powershell -ExecutionPolicy Bypass -File experiments\run-recovery.ps1 -NodeCount 4
powershell -ExecutionPolicy Bypass -File experiments\run-recovery.ps1 -NodeCount 5
```

## Notes

- All scripts use deterministic seeds and sequential key order where the
  metric needs exactness, so re-running them reproduces the same numbers.
- Throughput and latency carry run-to-run machine noise (debug build,
  localhost); structural numbers (keys moved, hit rates, memory, replication
  gap, detection/recovery times) are stable.
- The client now bounds TCP connects at 250 ms (`connection.rs`), so requests
  to a dead node fail fast on Windows instead of hanging ~2 s per attempt;
  the failure experiment depends on this.
- Nodes run on high, isolated port ranges (28001+, 28201+, 7201+, 7301+,
  7401+, 7501+) and are stopped by the script on completion or failure.
