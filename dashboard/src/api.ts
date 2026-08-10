import { nodeLabel } from "./config";

export interface LatencySummary {
  count: number;
  avg_us: number;
  p50_us: number;
  p95_us: number;
  p99_us: number;
  max_us: number;
}

export interface NodeSnapshot {
  node: string;
  uptime_secs: number;
  recovery_ms: number;
  requests: {
    total: number;
    gets: number;
    sets: number;
    deletes: number;
    hits: number;
    misses: number;
    hit_rate: number;
  };
  rates: { total: number; get: number; set: number; delete: number };
  latency: {
    ping: LatencySummary;
    get: LatencySummary;
    set: LatencySummary;
    delete: LatencySummary;
    info: LatencySummary;
  };
  storage: {
    keys: number;
    used_bytes: number;
    max_bytes: number;
    evictions: number;
    ttl_expirations: number;
  };
  aof: { bytes_written: number; write_count: number; fsync_count: number; rewrite_count: number } | null;
  replication: { sent: number; failed: number; received: number };
  peers: { alive: number; suspected: number; failed: number } | null;
}

export interface NodeState {
  url: string;
  label: string;
  ok: boolean;
  snapshot: NodeSnapshot | null;
}

export async function fetchNode(url: string): Promise<NodeSnapshot> {
  const response = await fetch(url, { signal: AbortSignal.timeout(1500) });
  if (!response.ok) {
    throw new Error(`HTTP ${response.status} from ${url}`);
  }
  return (await response.json()) as NodeSnapshot;
}

export async function pollNodes(urls: string[]): Promise<NodeState[]> {
  const results = await Promise.allSettled(urls.map((url) => fetchNode(url)));
  return urls.map((url, index) => {
    const settled = results[index];
    return {
      url,
      label: nodeLabel(url),
      ok: settled.status === "fulfilled",
      snapshot: settled.status === "fulfilled" ? settled.value : null,
    };
  });
}

export function aggregate(nodes: NodeState[]) {
  const up = nodes.filter((n) => n.ok);
  const reqPerSec = up.reduce((sum, n) => sum + (n.snapshot?.rates.total ?? 0), 0);
  const hits = up.reduce((sum, n) => sum + (n.snapshot?.requests.hits ?? 0), 0);
  const misses = up.reduce((sum, n) => sum + (n.snapshot?.requests.misses ?? 0), 0);
  const total = hits + misses;
  const hitRate = total > 0 ? hits / total : 0;
  const p99 = Math.max(0, ...up.map((n) => n.snapshot?.latency.get.p99_us ?? 0));
  const p50 = Math.max(0, ...up.map((n) => n.snapshot?.latency.get.p50_us ?? 0));
  const p95 = Math.max(0, ...up.map((n) => n.snapshot?.latency.get.p95_us ?? 0));
  return { up, reqPerSec, hitRate, p50, p95, p99 };
}