# CacheX dashboard

Real-time observability UI for a CacheX cluster. Polls each node's `/metrics`
endpoint over HTTP and renders KPIs, charts, and a per-node table.

## Prerequisites

- Node.js (npm) for the dashboard itself.
- The Rust server built (`cargo build -p cachex-server`), and a running cluster.

## Running the dashboard

1. Start a 3-node cluster. The stock configs already enable `[metrics]`:

   ```powershell
   cargo run -p cachex-server -- configs/node1.toml
   cargo run -p cachex-server -- configs/node2.toml
   cargo run -p cachex-server -- configs/node3.toml
   ```

   Each node must have `[metrics] enabled = true` in its config; the defaults
   assume metrics on 127.0.0.1:9001-9003.

2. Install and start the dev server:

   ```powershell
   npm install
   npm run dev
   ```

3. Open http://localhost:5173.

The dashboard polls every 2 s and keeps the last 120 samples per node.

## Node metrics endpoints

Override which endpoints the dashboard polls (comma-separated, full URLs):

```powershell
$env:VITE_METRICS_URLS = "http://127.0.0.1:9001/metrics,http://127.0.0.1:9002/metrics"
npm run dev
```

Without the override, the defaults are `http://127.0.0.1:9001/metrics`,
`...9002/metrics`, and `...9003/metrics` (see `src/config.ts`).

## Production build

```powershell
npm run build
```

Output goes to `dist/` and can be served by any static host.
