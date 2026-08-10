

import type { NodeState } from "../api";
import { formatBytes, formatDuration, formatRate } from "./StatCard";

export function NodeTable({ nodes }: { nodes: NodeState[] }) {
  return (
    <div className="node-table-wrap">
      <div className="chart-title">Nodes</div>
      <table className="node-table">
        <thead>
          <tr>
            <th>Node</th>
            <th>Status</th>
            <th>Memory</th>
            <th>Req/s</th>
            <th>Hit rate</th>
            <th>Keys</th>
            <th>Evictions</th>
            <th>Uptime</th>
          </tr>
        </thead>
        <tbody>
          {nodes.map((node) => {
            const s = node.snapshot;
            const memPct = s && s.storage.max_bytes > 0 ? (s.storage.used_bytes / s.storage.max_bytes) * 100 : 0;
            const hitRate = s && s.requests.hits + s.requests.misses > 0
              ? (s.requests.hits / (s.requests.hits + s.requests.misses)) * 100
              : 0;
            return (
              <tr key={node.url} className={node.ok ? "" : "row-down"}>
                <td className="mono">{node.label}</td>
                <td>
                  <span className={`pill ${node.ok ? "pill-good" : "pill-bad"}`}>
                    {node.ok ? "Healthy" : "Down"}
                  </span>
                </td>
                <td className="mono">
                  {s
                    ? `${memPct.toFixed(0)}% (${formatBytes(s.storage.used_bytes)})`
                    : "-"}
                </td>
                <td className="mono">{s ? formatRate(s.rates.total) : "-"}</td>
                <td className="mono">{s ? `${hitRate.toFixed(1)}%` : "-"}</td>
                <td className="mono">{s ? s.storage.keys.toLocaleString() : "-"}</td>
                <td className="mono">{s ? s.storage.evictions.toLocaleString() : "-"}</td>
                <td className="mono">{s ? formatDuration(s.uptime_secs) : "-"}</td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}