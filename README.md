# CacheX

A distributed in-memory cache written in Rust. Like Redis but simpler.

CacheX shards keys across a cluster of nodes with client-side consistent hashing, keeps every node's data in memory with O(1) LRU eviction, replicates it across replicas for fault tolerance, and streams live per-node metrics to a real-time dashboard over SSE.

## Features

- Thread-safe LRU cache with O(1) get, put, and eviction
- Precise TTL expiry with a background reaper
- Client-side consistent hashing with virtual nodes to minimize key movement on join/leave
- Configurable primary-replica replication
- Heartbeat failure detection with automatic AOF recovery; no single point of failure
- Custom binary TCP protocol with connection pooling
- Live metrics API plus a real-time React dashboard over an SSE-backed control API
- TOML config files for nodes, cluster topology, and runtime behavior

## Contents

- [Features](#features-1)
- [Performance / Benchmarking](#performance--benchmarking)
- [Testing](#testing)
- [Set Up and Usage](#set-up-and-usage)
- [Examples](#examples)
- [Contributing](#contributing)

## Features

### O(1) LRU cache with TTL

A `HashMap` key index paired with an intrusive doubly linked list gives get, put, and eviction all in O(1) time. A configurable memory cap (`max_memory_bytes`) bounds the resident set, and when it is reached the least-recently-used key is evicted.

Keys can be written with a TTL. A background reaper purges expired entries on a fixed interval, so memory is reclaimed without a scan on the hot path.

### Consistent hashing and fault tolerance

The client routes each key through a consistent hash ring with virtual nodes (default 100 per server) instead of simple modulo. When a node joins or leaves the cluster, only a small fraction of keys are remapped. Measured on a 100K-key keyspace across a 3-to-4-node resize, consistent hashing reduced key movement from **74.9%** (modulo) to **26.0%**.

### Replication

Each key is written to the primary node and, when replication is enabled (`factor` > 1), to that node's replica as well. Replication is async, so writes do not block on replicas, and the cluster tracks the replication gap between primary and replica. Measured with a replication factor of 2 across a 3-node cluster: roughly **2x** the keys and memory versus RF=1, at the cost of about **48%** lower write throughput and **98%** higher memory usage.

### Failure detection and recovery

Every node sends a heartbeat every second. When a node misses the configured number of heartbeats (default 2), it is marked down in roughly **2 seconds** (measured 1.8 to 2.1s across 3, 4, and 5-node clusters). Clients update their hash ring and stop routing to the dead node, and replication/failover continues.

Recovery is automatic. When a node comes back, it loads its append-only file (AOF), replays it in about **0.2 seconds** (measured 174 to 186ms), and rejoins the ring. In failure tests across 3, 4, and 5-node clusters, a healthy node recovered in **under 1.5 seconds** with zero data loss and zero errors after recovery.

### Protocol and client

Nodes speak a hand-built binary TCP protocol over the public port, with a separate internal port (public + 1000) for cluster traffic. The client crate provides a typed API with connection pooling, so a client can reuse connections across requests instead of opening one per command.

```rust
use cachex_client::CachexClient;

let client = CachexClient::consistent(vec![
    "127.0.0.1:7001".into(),
    "127.0.0.1:7002".into(),
    "127.0.0.1:7003".into(),
], 100);

client.set("user:1".into(), b"jill".to_vec(), Some(60)).await?;
let value = client.get("user:1").await?;
client.delete("user:1").await?;
```

### Observability

Every node exposes a metrics endpoint (default port 9001-9003) that returns current throughput, hit rate, latency percentiles, keys, memory usage, evictions, and replication gap. A control service (default port 9100) aggregates these and streams them to the dashboard over an SSE stream at `/control/events`. The React dashboard in `dashboard/` polls metrics every 2 seconds and keeps the last 120 samples per chart, so you can watch a cluster respond to load, eviction, and failures live.

## Performance / Benchmarking

`cachex-bench` (in `crates/cachex-bench`) drives a cluster with configurable concurrency, keyspace, value size, and GET/SET mix, and writes results to CSV.

```powershell
cargo run --release -p cachex-bench -- `
  --nodes 127.0.0.1:7001,127.0.0.1:7002,127.0.0.1:7003 `
  --clients 64 --requests 200000 --keys 100000 `
  --value-size 128 --get-ratio 0.9 --output bench.csv
```

Measured results (all reproducible with the scripts in `experiments/`):

| Scenario                       | Result                                                                                                                                                                                                              |
| ------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Eviction under memory pressure | Hit rate fell from **73.4%** to **25.7%** as the working set grew 7K to 20K keys against a 1 MiB cache (resident set 5,140 keys)                                                                                    |
| TTL expiry                     | Hit rate fell from **100%** to **0%** across the ~10s TTL boundary; expired keys reclaimed by the reaper                                                                                                            |
| Consistent hashing             | Key movement reduced from **74.9%** (modulo) to **26.0%** on a 3-to-4-node resize of a 100K-key keyspace                                                                                                            |
| Replication RF=2               | **2x** keys and memory, **~48%** lower throughput, **98%** higher memory usage versus RF=1                                                                                                                          |
| Failure detection              | Dead node flagged in **~2s** (1.8 to 2.1s) across 3, 4, and 5-node clusters                                                                                                                                         |
| Recovery                       | Healthy node recovered in **<1.5s** (0.8 to 1.0s), AOF replay **~0.2s**, zero data loss, zero post-recovery errors                                                                                                  |
| Scalability                    | Throughput flat from 1 to 5 nodes (client-limited at ~16.5K to 22.9K ops/s with 8 clients); at 1,000 clients, throughput held at ~19K to 25K ops/s with latency rising from 0.5ms to 27.7ms (P99 from 2ms to 100ms) |

## Testing

The workspace runs **102 tests** across all five crates:

```powershell
cargo test --workspace
```

Test coverage includes the LRU cache (eviction, touch-prevention, memory bounds), consistent hashing ring (movement and distribution), TTL reaper, replication gap tracking, heartbeat failure detection, AOF replay, and client routing.

### Scenario scripts

`scripts/scenarios/` has one PowerShell script per behavior, each designed to be run while the dashboard is open so you can watch the charts react live. Every script starts its own nodes on ports 7001-7003, so only the dashboard needs to be running.

| #   | Script                        | What it shows                                                 | Cluster used                    |
| --- | ----------------------------- | ------------------------------------------------------------- | ------------------------------- |
| 1   | `scenario-01-load.ps1`        | Throughput, hit rate, GET latency under read-heavy load       | stock 100 MiB                   |
| 2   | `scenario-02-write.ps1`       | Keys and memory climbing under write-heavy load               | stock 100 MiB                   |
| 3   | `scenario-03-hitrate.ps1`     | Warm-up then hit rate climbing to ~100%                       | stock 100 MiB                   |
| 4   | `scenario-04-eviction.ps1`    | Memory pinned at 100%, evictions climbing, hit rate low       | 1 MiB (swapped, restored after) |
| 5   | `scenario-05-ttl.ps1`         | Keys/memory expiring after a 10s TTL, reads then miss         | stock 100 MiB                   |
| 6   | `scenario-06-failure.ps1`     | Node 2 goes red "Down", Nodes KPI drops to 2/3, then recovers | stock 100 MiB                   |
| 7   | `scenario-07-replication.ps1` | Keys + memory ~2x under RF=2                                  | RF=2 (swapped, restored after)  |
| 8   | `scenario-08-scalability.ps1` | Three throughput bumps at 4/16/64 clients, latency rising     | stock 100 MiB                   |

### Experiments

`experiments/` contains reproducible, scripted experiments for eviction, TTL, hashing, replication, scalability, and failure/recovery (the last parameterized with `-NodeCount` for 3, 4, or 5 nodes). Each writes CSV results to `experiments/results/` and its own notes to `experiments/README.md`.

## Set Up and Usage

### Prerequisites

- Rust toolchain (edition 2021, `tokio` 1.x)
- Node.js for the dashboard
- Windows PowerShell for the helper scripts

### Build

```powershell
cargo build --release
```

This builds the server (`cachex-server`), the workload generator (`cachex-bench`), the control service (`cachex-control`), and the client library. Individual crates build with `cargo build -p cachex-server`, etc.

### Config file

Each node reads a TOML config. `configs/node1.toml` is a complete example:

```toml
[node]                      # this node's identity and public address
id = 1
host = "127.0.0.1"
port = 7001

[cluster]                   # static cluster topology (genesis nodes)
nodes = [
    "127.0.0.1:7001",
    "127.0.0.1:7002",
    "127.0.0.1:7003",
]

[cache]                     # memory cap and eviction policy
max_memory_bytes = 104857600
eviction_policy = "lru"
ttl_purge_interval_secs = 1

[aof]                       # append-only file persistence
enabled = false
path = "node1.aof"
fsync = "interval"
fsync_interval_secs = 1
rewrite_threshold_bytes = 67108864

[hashing]                   # virtual nodes per server on the consistent hash ring
vnodes = 100

[replication]               # async primary-replica replication
enabled = false
factor = 1

[heartbeat]                 # failure detection timing
interval_secs = 1
timeout_ms = 500
miss_threshold = 2

[metrics]                   # per-node metrics API
enabled = true
host = "127.0.0.1"
port = 9001
```

The three stock configs `configs/node1.toml`, `configs/node2.toml`, and `configs/node3.toml` form a 3-node cluster on ports 7001-7003 with metrics on 9001-9003.

### Run the cluster

The simplest way to get a 3-node cluster up is the helper script:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/start-cluster.ps1
```

Or start nodes individually:

```powershell
cargo run --release -p cachex-server -- configs/node1.toml
cargo run --release -p cachex-server -- configs/node2.toml
cargo run --release -p cachex-server -- configs/node3.toml
```

Each node prints an AOF replay report (if persistence is enabled), its public/internal addresses, and confirms it is listening before serving.

### Run the dashboard

```powershell
cd dashboard
npm install
npm run dev
```

Open http://localhost:5173. The dashboard polls each node's metrics endpoint every 2 seconds and streams live events from the control service.

To run the control service on its own (default port 9100):

```powershell
cargo run --release -p cachex-control -- --root .
```

## Examples

### Example 1: Cluster + workload in one terminal

```powershell
# Terminal 1 - start the cluster
cargo run --release -p cachex-server -- configs/node1.toml

# Terminal 2 - drive 100K requests across 3 nodes
cargo run --release -p cachex-bench -- `
  --nodes 127.0.0.1:7001,127.0.0.1:7002,127.0.0.1:7003 `
  --clients 32 --requests 100000 --keys 50000 `
  --get-ratio 0.9 --output bench.csv
```

### Example 2: cachex-bench reference

```
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
```

### Example 3: Watch eviction live in the dashboard

With the dashboard open, run:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/scenarios/scenario-04-eviction.ps1
```

The script swaps in a 1 MiB cache config, floods the cluster, and prints hints telling you which chart or KPI to watch (memory pinned at 100%, evictions climbing, hit rate falling). When it finishes it restores the stock cluster.

### Example 4: CacheX as a library

Add `cachex-client` to your project and talk to the cluster from Rust (full example under [Protocol and client](#protocol-and-client)).
