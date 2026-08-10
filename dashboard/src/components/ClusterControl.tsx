

import { useState } from "react";
import type { NodeState } from "../api";
import {
  killNode,
  restartNode,
  scaleCluster,
  setReplication,
  startCluster,
  stopCluster,
  type ClusterStatus,
} from "../control";
import { publicPort } from "../control";

export function ClusterControl({
  status,
  nodes,
  onClusterStatus,
  disabled,
}: {
  status: ClusterStatus | null;
  nodes: NodeState[];
  onClusterStatus: (status: ClusterStatus) => void;
  disabled?: boolean;
}) {
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const run = async (key: string, action: () => Promise<unknown>) => {
    if (busy) return;
    setBusy(key);
    setError(null);
    try {
      const result = await action();
      if (result && typeof result === "object" && "spec" in (result as object)) {
        onClusterStatus(result as ClusterStatus);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  };

  const ready = status?.ready ?? false;
  const spec = status?.spec;
  const upByPort = new Map(nodes.filter((n) => n.ok).map((n) => [metricsPortOf(n.url), true]));

  return (
    <div className="control-panel">
      {error && <div className="action-error">{error}</div>}

      <section className="panel-section">
        <div className="chart-title">Cluster</div>
        <div className="btn-row">
          <button
            className="btn btn-primary"
            disabled={disabled || !!busy || !ready}
            onClick={() => run("start", () => startCluster())}
          >
            {busy === "start" ? "Starting…" : "Start Cluster"}
          </button>
          <button
            className="btn btn-danger"
            disabled={disabled || !!busy}
            onClick={() => run("stop", () => stopCluster())}
          >
            {busy === "stop" ? "Stopping…" : "Stop Cluster"}
          </button>
        </div>
      </section>

      {spec && (
        <>
          <section className="panel-section">
            <div className="chart-title">Nodes</div>
            <div className="node-control-list">
              {status!.nodes.map((info) => {
                const up = upByPort.has(info.metrics_port);
                return (
                  <div key={info.id} className="node-control-row">
                    <span className={`pill ${up ? "pill-good" : "pill-bad"}`}>
                      {up ? "Healthy" : "Down"}
                    </span>
                    <span className="mono node-control-port">{publicPort(info.address)}</span>
                    <span className="node-control-meta mono">
                      {info.pid ? `pid ${info.pid}` : "no process"}
                    </span>
                    <div className="btn-row right">
                      <button
                        className="btn btn-sm btn-warn"
                        disabled={disabled || !!busy}
                        onClick={() => run(`kill-${info.id}`, () => killNode(info.id))}
                      >
                        {busy === `kill-${info.id}` ? "Killing…" : "Kill"}
                      </button>
                      <button
                        className="btn btn-sm"
                        disabled={disabled || !!busy}
                        onClick={() => run(`restart-${info.id}`, () => restartNode(info.id))}
                      >
                        {busy === `restart-${info.id}` ? "Restarting…" : "Restart"}
                      </button>
                    </div>
                  </div>
                );
              })}
            </div>
          </section>

          <section className="panel-section">
            <div className="chart-title">
              Scale cluster <span className="muted mono">currently {spec.node_count} nodes</span>
            </div>
            <div className="btn-row">
              {[3, 4, 5].map((n) => (
                <button
                  key={n}
                  className={`btn btn-sm ${spec.node_count === n ? "btn-active" : ""}`}
                  disabled={disabled || !!busy || spec.node_count === n}
                  onClick={() => run(`scale-${n}`, () => scaleCluster(n))}
                >
                  {n} Nodes
                </button>
              ))}
            </div>
            <div className="panel-hint">Scaling restarts the whole cluster (config is read at boot).</div>
          </section>

          <section className="panel-section">
            <div className="chart-title">
              Replication factor <span className="muted mono">currently RF={spec.replication_factor}</span>
            </div>
            <div className="btn-row">
              {[1, 2, 3].map((f) => (
                <button
                  key={f}
                  className={`btn btn-sm ${spec.replication_factor === f ? "btn-active" : ""}`}
                  disabled={disabled || !!busy || spec.replication_factor === f}
                  onClick={() => run(`rf-${f}`, () => setReplication(f))}
                >
                  RF = {f}
                </button>
              ))}
            </div>
            <div className="panel-hint">
              RF=1 keeps one copy (fastest). RF=2 adds write amplification to test replication.
            </div>
          </section>
        </>
      )}
    </div>
  );
}

function metricsPortOf(url: string): number {
  const match = url.match(/:(\d+)\/metrics$/);
  return match ? Number(match[1]) : 0;
}