

import type { NodeState } from "../api";
import type { ClusterStatus } from "../control";
import { formatBytes, formatRate } from "./StatCard";

export function ClusterMap({ status, nodes }: { status: ClusterStatus | null; nodes: NodeState[] }) {
  if (!status) {
    return <div className="cluster-map-empty">control service offline — no cluster spec</div>;
  }
  const byMetricsPort = new Map(nodes.map((n) => [metricsPortOf(n.url), n]));

  return (
    <div className="cluster-map">
      {status.nodes.map((info) => {
        const state = byMetricsPort.get(info.metrics_port);
        const ok = state?.ok ?? false;
        const s = state?.snapshot;
        const hitRate =
          s && s.requests.hits + s.requests.misses > 0
            ? (s.requests.hits / (s.requests.hits + s.requests.misses)) * 100
            : 0;
        return (
          <div key={info.id} className={`cluster-node ${ok ? "up" : "down"}`}>
            <div className="cluster-node-head">
              <span className="mono cluster-node-port">{info.public_port}</span>
              <span className={`pill ${ok ? "pill-good" : "pill-bad"}`}>{ok ? "Healthy" : "Down"}</span>
            </div>
            <div className="cluster-node-body">
              {s ? (
                <>
                  <div className="mono node-metric">{formatRate(s.rates.total)} req/s</div>
                  <div className="mono node-metric">{formatBytes(s.storage.used_bytes)}</div>
                  <div className="mono node-metric">{hitRate.toFixed(1)}% hit</div>
                </>
              ) : (
                <div className="mono node-metric muted">no metrics</div>
              )}
            </div>
          </div>
        );
      })}
    </div>
  );
}

function metricsPortOf(url: string): number {
  const match = url.match(/:(\d+)\/metrics$/);
  return match ? Number(match[1]) : 0;
}