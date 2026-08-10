# CacheX dashboard scenario scripts

One script per behavior. Each drives the cluster while the dashboard is open,
so you can watch the numbers and charts react live.

## Setup (once)

```powershell
cargo build -p cachex-server
cargo build -p cachex-bench
```

Then start the dashboard (in `dashboard\`):

```powershell
npm install
npm run dev
```

Open http://localhost:5173. The dashboard polls `http://127.0.0.1:9001-9003/metrics`
every 2 seconds and keeps the last 120 samples per chart.

## Running the scenarios

Every script starts/restarts its own nodes (on ports 7001-7003 / metrics
9001-9003), so you do **not** need `start-cluster.ps1` beforehand - just have
the dashboard open. Run each from the repo root:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/scenarios/scenario-01-load.ps1
```

| # | Script | What it shows | Cluster used |
|---|--------|---------------|--------------|
| 1 | `scenario-01-load.ps1` | Throughput, hit rate, GET latency under read-heavy load | stock 100 MiB |
| 2 | `scenario-02-write.ps1` | Keys and memory climbing under write-heavy load | stock 100 MiB |
| 3 | `scenario-03-hitrate.ps1` | Warm-up then hit rate climbing to ~100% | stock 100 MiB |
| 4 | `scenario-04-eviction.ps1` | Memory pinned at 100%, evictions climbing, hit rate low | 1 MiB (swapped, restored after) |
| 5 | `scenario-05-ttl.ps1` | Keys/memory expiring after a 10s TTL, reads then miss | stock 100 MiB |
| 6 | `scenario-06-failure.ps1` | Node 2 goes red "Down", Nodes KPI drops to 2/3, then recovers to 3/3 | stock 100 MiB |
| 7 | `scenario-07-replication.ps1` | Keys + memory ~2x under RF=2 | RF=2 (swapped, restored after) |
| 8 | `scenario-08-scalability.ps1` | Three throughput bumps at 4/16/64 clients, latency rising | stock 100 MiB |

Scenarios 4 and 7 swap in special configs (`configs/scenario/`) and restore the
stock cluster when they finish.

Each scenario prints `>> WATCH THE DASHBOARD:` hints as it runs, telling you
exactly which chart or KPI to look at.

## Notes

- Bench CSV results are written to `scripts/scenarios/results/`.
- The metrics API requires `[metrics] enabled = true` in each node config; all
  stock and scenario configs enable it.
- Scenario scripts log node stdout/stderr to `scripts/scenarios/node*.log` files.
