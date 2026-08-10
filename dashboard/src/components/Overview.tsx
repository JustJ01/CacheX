

import type { NodeState } from "../api";
import { aggregate } from "../api";
import type { ClusterStatus } from "../control";
import { HitRateChart, LatencyChart, ThroughputChart, type Sample } from "./Charts";
import { ClusterMap } from "./ClusterMap";
import { NodeTable } from "./NodeTable";
import { formatRate, formatUs, StatCard } from "./StatCard";

export function Overview({
  status,
  nodes,
  samples,
}: {
  status: ClusterStatus | null;
  nodes: NodeState[];
  samples: Sample[];
}) {
  const agg = aggregate(nodes);
  const total = status?.spec.node_count ?? 3;

  return (
    <div className="overview">
      <section className="kpi-grid">
        <StatCard
          label="Nodes"
          value={`${agg.up.length} / ${total}`}
          sub={agg.up.length === total ? "all healthy" : "degraded"}
          tone={agg.up.length === total ? "good" : "bad"}
        />
        <StatCard label="Req/sec" value={formatRate(agg.reqPerSec)} sub="aggregate" tone="accent" />
        <StatCard
          label="Hit rate"
          value={`${(agg.hitRate * 100).toFixed(1)}%`}
          sub="weighted across nodes"
          tone="default"
        />
        <StatCard label="P99" value={formatUs(agg.p99)} sub="GET, worst node" />
      </section>

      <section className="panel-section map-section">
        <div className="chart-title">Live cluster</div>
        <ClusterMap status={status} nodes={nodes} />
      </section>

      <section className="chart-grid">
        <ThroughputChart samples={samples} />
        <HitRateChart samples={samples} />
        <LatencyChart samples={samples} />
      </section>

      <section className="node-section">
        <NodeTable nodes={nodes.length > 0 ? nodes : []} />
      </section>
    </div>
  );
}