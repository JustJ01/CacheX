

import { useEffect, useState } from "react";
import {
  CartesianGrid,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import {
  runEvictionExperiment,
  runFailureExperiment,
  runHashExperiment,
  runReplicationExperiment,
  runScalabilityExperiment,
  runTtlExperiment,
  type EvictionResult,
  type FailureResult,
  type HashingResult,
  type ReplicationResult,
  type ScalabilityResult,
  type TtlResult,
  type LoadParams,
} from "../control";

function RatioBar({ pct, color, label }: { pct: number; color: string; label: string }) {
  return (
    <div className="ratio-row">
      <span className="ratio-label">{label}</span>
      <div className="ratio-track">
        <div className="ratio-fill" style={{ width: `${Math.min(100, pct)}%`, background: color }} />
      </div>
      <span className="ratio-value mono">{pct.toFixed(1)}%</span>
    </div>
  );
}

function formatBytes(bytes: number): string {
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${bytes} B`;
}

function formatRate(rate: number): string {
  if (rate >= 1000) return `${(rate / 1000).toFixed(1)}K`;
  return rate.toFixed(0);
}

function Finding({ text }: { text: string }) {
  return (
    <div className="exp-finding">
      <strong>Finding:</strong> {text}
    </div>
  );
}

function ErrorBox({ message }: { message: string }) {
  return <div className="action-error">{message}</div>;
}

const CHART_AXIS = { fill: "#94A3B8", fontSize: 11 };
const CHART_GRID = "rgba(148,163,184,0.12)";
const tooltipStyle = () => ({
  contentStyle: {
    background: "#12182B",
    border: "1px solid #1E2A4A",
    borderRadius: 8,
    fontSize: 12,
    fontFamily: "'Fira Code', monospace",
  },
  labelStyle: { color: "#E2E8F0" },
});

function EvictionCard() {
  const [capacityMb, setCapacityMb] = useState(1);
  const [workingSet, setWorkingSet] = useState(20_000);
  const [valueSize, setValueSize] = useState(128);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<EvictionResult | null>(null);

  const run = async () => {
    setBusy(true);
    setError(null);
    setResult(null);
    try {
      setResult(
        await runEvictionExperiment({
          capacity_bytes: Math.round(capacityMb * 1024 * 1024),
          working_set: workingSet,
          value_size: valueSize,
        }),
      );
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="exp-card">
      <div className="exp-title">LRU Eviction</div>
      <div className="exp-blurb">
        Fill the cache past its memory limit and watch resident keys, evictions, and hit rate move.
        Runs in-process against the real cache store.
      </div>
      <div className="field-row">
        <label className="field">
          <span className="field-label">Capacity (MiB)</span>
          <input className="mono field-input" type="number" min={0.1} step={0.1} value={capacityMb} onChange={(e) => setCapacityMb(Number(e.target.value))} />
        </label>
        <label className="field">
          <span className="field-label">Working set</span>
          <input className="mono field-input" type="number" min={100} value={workingSet} onChange={(e) => setWorkingSet(Number(e.target.value))} />
        </label>
        <label className="field">
          <span className="field-label">Value bytes</span>
          <input className="mono field-input" type="number" min={1} value={valueSize} onChange={(e) => setValueSize(Number(e.target.value))} />
        </label>
        <button className="btn btn-primary" disabled={busy} onClick={() => void run()}>
          {busy ? "Running…" : "Run LRU Experiment"}
        </button>
      </div>
      {busy && <div className="panel-hint">Filling + probing the cache… progress streams to the timeline.</div>}
      {error && <ErrorBox message={error} />}
      {result && (
        <div className="exp-result">
          <div className="load-run-stats">
            <div className="stat"><span>Capacity</span><b className="mono">{formatBytes(result.capacity_bytes)}</b></div>
            <div className="stat"><span>Working set</span><b className="mono">{result.working_set.toLocaleString()}</b></div>
            <div className="stat"><span>Resident keys</span><b className="mono">{result.resident_keys.toLocaleString()}</b></div>
            <div className="stat"><span>Evictions</span><b className="mono">{result.evictions.toLocaleString()}</b></div>
            <div className="stat"><span>Hit rate</span><b className="mono">{result.hit_rate.toFixed(1)}%</b></div>
          </div>
          <div className="chart-wrap">
            <div className="chart-title">Hit rate vs working set size</div>
            <ResponsiveContainer width="100%" height={160}>
              <LineChart data={result.curve} margin={{ top: 8, right: 8, left: 0, bottom: 0 }}>
                <CartesianGrid strokeDasharray="3 3" stroke={CHART_GRID} />
                <XAxis dataKey="keys" stroke={CHART_AXIS.fill} tick={CHART_AXIS} tickFormatter={(v: number) => `${formatRate(v)}K`.replace("K", "k")} />
                <YAxis stroke={CHART_AXIS.fill} tick={CHART_AXIS} width={40} domain={[0, 100]} tickFormatter={(v: number) => `${v}%`} />
                <Tooltip {...tooltipStyle()} formatter={(v: number) => [`${v.toFixed(1)}%`, "hit rate"]} />
                <Line type="monotone" dataKey="hit_rate" stroke="#22C55E" strokeWidth={2} dot={{ r: 3 }} />
              </LineChart>
            </ResponsiveContainer>
          </div>
          <Finding text="Increasing the working set beyond cache capacity increases eviction pressure and reduces cache hit rate." />
        </div>
      )}
    </div>
  );
}

function TtlCard() {
  const [keys, setKeys] = useState(5_000);
  const [ttlSecs, setTtlSecs] = useState(10);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<TtlResult | null>(null);
  const [progress, setProgress] = useState(0);

  useEffect(() => {
    if (!busy) return;
    setProgress(0);
    const id = window.setInterval(() => setProgress((p) => p + 1), 1000);
    return () => window.clearInterval(id);
  }, [busy]);

  const run = async () => {
    setBusy(true);
    setError(null);
    setResult(null);
    try {
      setResult(await runTtlExperiment({ keys, ttl_secs: ttlSecs }));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const total = ttlSecs || 1;
  const lastElapsed = result && result.samples.length > 0 ? result.samples[result.samples.length - 1].elapsed_secs : 0;
  const pct = Math.min(100, ((busy ? progress : lastElapsed) / total) * 100);

  return (
    <div className="exp-card">
      <div className="exp-title">TTL Expiry</div>
      <div className="exp-blurb">
        Write keys with a short TTL and watch the reaper sweep them out while hit rate collapses.
      </div>
      <div className="field-row">
        <label className="field">
          <span className="field-label">Keys</span>
          <input className="mono field-input" type="number" min={100} value={keys} onChange={(e) => setKeys(Number(e.target.value))} />
        </label>
        <label className="field">
          <span className="field-label">TTL (secs)</span>
          <input className="mono field-input" type="number" min={2} value={ttlSecs} onChange={(e) => setTtlSecs(Number(e.target.value))} />
        </label>
        <button className="btn btn-primary" disabled={busy} onClick={() => void run()}>
          {busy ? "Running…" : "Run TTL Experiment"}
        </button>
      </div>
      {busy && (
        <div className="exp-result">
          <div className="chart-title">TTL countdown</div>
          <div className="ratio-track" style={{ height: 14 }}>
            <div className="ratio-fill" style={{ width: `${pct}%`, background: "#D97706" }} />
          </div>
          <div className="panel-hint mono">
            {progress} / {total} sec elapsed — keys expiring
          </div>
        </div>
      )}
      {error && <ErrorBox message={error} />}
      {result && (
        <div className="exp-result">
          <div className="ratio-track" style={{ height: 14 }}>
            <div className="ratio-fill" style={{ width: `${pct}%`, background: "#D97706" }} />
          </div>
          <div className="panel-hint mono">
            {lastElapsed} / {result.ttl_secs} sec — {result.total_expired.toLocaleString()} of{" "}
            {result.keys.toLocaleString()} keys expired
          </div>
          <div className="node-table-wrap">
            <table className="node-table">
              <thead>
                <tr><th>Elapsed</th><th>Remaining</th><th>Hit rate</th></tr>
              </thead>
              <tbody>
                {result.samples.map((s) => (
                  <tr key={s.elapsed_secs}>
                    <td className="mono">{s.elapsed_secs}s</td>
                    <td className="mono">{s.remaining.toLocaleString()}</td>
                    <td className="mono">{s.hit_rate.toFixed(1)}%</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <Finding text="Once a key's TTL elapses it is gone for good — hit rate drops to zero the moment the last entry is reaped." />
        </div>
      )}
    </div>
  );
}

function ReplicationCard() {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<ReplicationResult | null>(null);

  const run = async () => {
    setBusy(true);
    setError(null);
    setResult(null);
    try {
      setResult(await runReplicationExperiment({}));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const cmp = result?.comparison;
  const rows = result?.rows ?? [];

  return (
    <div className="exp-card exp-wide">
      <div className="exp-title">Replication</div>
      <div className="exp-blurb">
        The control service restarts the cluster at RF=1 and RF=2, runs the identical workload at each,
        and compares throughput, memory, and replication counters.
      </div>
      <button className="btn btn-primary" disabled={busy} onClick={() => void run()}>
        {busy ? "Running…" : "Run Replication Experiment"}
      </button>
      {busy && <div className="panel-hint">Restarting the cluster twice and running a 50k-request workload per factor…</div>}
      {error && <ErrorBox message={error} />}
      {result && rows.length >= 2 && cmp && (
        <div className="exp-result">
          <div className="node-table-wrap">
            <table className="node-table">
              <thead>
                <tr><th></th><th>RF = 1</th><th>RF = 2</th></tr>
              </thead>
              <tbody>
                <tr>
                  <td>Throughput</td>
                  <td className="mono">{formatRate(rows[0].report.ops_per_sec)} ops/s</td>
                  <td className="mono">{formatRate(rows[1].report.ops_per_sec)} ops/s</td>
                </tr>
                <tr>
                  <td>Memory</td>
                  <td className="mono">{formatBytes(rows[0].used_bytes)}</td>
                  <td className="mono">{formatBytes(rows[1].used_bytes)}</td>
                </tr>
                <tr>
                  <td>Copies per key</td>
                  <td className="mono">{rows[0].copies}</td>
                  <td className="mono">{rows[1].copies}</td>
                </tr>
                <tr>
                  <td>Replication ops</td>
                  <td className="mono">{rows[0].repl_sent.toLocaleString()}</td>
                  <td className="mono">{rows[1].repl_sent.toLocaleString()}</td>
                </tr>
                <tr>
                  <td>Errors</td>
                  <td className="mono">{rows[0].report.errors}</td>
                  <td className="mono">{rows[1].report.errors}</td>
                </tr>
              </tbody>
            </table>
          </div>
          <div className="load-run-stats">
            <div className="stat">
              <span>Throughput change</span>
              <b className={`mono ${cmp.throughput_change_pct < 0 ? "bad" : "muted"}`}>
                {cmp.throughput_change_pct > 0 ? "+" : ""}{cmp.throughput_change_pct.toFixed(1)}%
              </b>
            </div>
            <div className="stat">
              <span>Memory change</span>
              <b className="mono">{cmp.memory_change_pct > 0 ? "+" : ""}{cmp.memory_change_pct.toFixed(1)}%</b>
            </div>
          </div>
          <Finding text="Replication improves redundancy at the cost of additional memory and write throughput." />
        </div>
      )}
    </div>
  );
}

function ScalabilityCard() {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<ScalabilityResult | null>(null);

  const run = async () => {
    setBusy(true);
    setError(null);
    setResult(null);
    try {
      setResult(await runScalabilityExperiment({}));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="exp-card exp-wide">
      <div className="exp-title">Scalability</div>
      <div className="exp-blurb">
        Same total workload, swept from 10 to 1000 clients. Throughput stops scaling once contention and
        queueing dominate.
      </div>
      <button className="btn btn-primary" disabled={busy} onClick={() => void run()}>
        {busy ? "Running…" : "Run Scalability Experiment"}
      </button>
      {busy && <div className="panel-hint">6 workloads of 30k requests at 10 → 1000 clients…</div>}
      {error && <ErrorBox message={error} />}
      {result && (
        <div className="exp-result">
          <div className="load-run-stats">
            <div className="stat">
              <span>Peak throughput</span>
              <b className="mono">{formatRate(result.peak.ops_per_sec ?? 0)} ops/s</b>
            </div>
            <div className="stat">
              <span>Peak at</span>
              <b className="mono">{result.peak.clients ?? 0} clients</b>
            </div>
            <div className="stat">
              <span>Rollover</span>
              <b className={`mono ${result.rollover ? "bad" : "muted"}`}>{result.rollover ? "yes" : "no"}</b>
            </div>
          </div>
          <div className="chart-wrap">
            <div className="chart-title">Throughput vs clients</div>
            <ResponsiveContainer width="100%" height={150}>
              <LineChart data={result.points} margin={{ top: 8, right: 8, left: 0, bottom: 0 }}>
                <CartesianGrid strokeDasharray="3 3" stroke={CHART_GRID} />
                <XAxis dataKey="clients" stroke={CHART_AXIS.fill} tick={CHART_AXIS} />
                <YAxis stroke={CHART_AXIS.fill} tick={CHART_AXIS} width={48} />
                <Tooltip {...tooltipStyle()} formatter={(v: number) => [`${Math.round(v).toLocaleString()}`, "ops/s"]} />
                <Line type="monotone" dataKey="ops_per_sec" stroke="#3B82F6" strokeWidth={2} dot={{ r: 3 }} />
              </LineChart>
            </ResponsiveContainer>
          </div>
          <div className="chart-wrap">
            <div className="chart-title">P99 latency vs clients</div>
            <ResponsiveContainer width="100%" height={150}>
              <LineChart data={result.points} margin={{ top: 8, right: 8, left: 0, bottom: 0 }}>
                <CartesianGrid strokeDasharray="3 3" stroke={CHART_GRID} />
                <XAxis dataKey="clients" stroke={CHART_AXIS.fill} tick={CHART_AXIS} />
                <YAxis stroke={CHART_AXIS.fill} tick={CHART_AXIS} width={48} tickFormatter={(v: number) => `${v}us`} />
                <Tooltip {...tooltipStyle()} formatter={(v: number) => [`${Math.round(v).toLocaleString()} us`, "p99"]} />
                <Line type="monotone" dataKey="p99_us" stroke="#EF4444" strokeWidth={2} dot={{ r: 3 }} />
              </LineChart>
            </ResponsiveContainer>
          </div>
          <Finding text="More clients don't necessarily mean more throughput; eventually contention and queueing dominate." />
        </div>
      )}
    </div>
  );
}

function FailureCard() {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<FailureResult | null>(null);

  const run = async () => {
    setBusy(true);
    setError(null);
    setResult(null);
    try {
      setResult(await runFailureExperiment({}));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const w = result?.workload;

  return (
    <div className="exp-card exp-wide">
      <div className="exp-title">Failure Recovery</div>
      <div className="exp-blurb">
        Warms the cluster, starts a live GET workload, kills node 7002, waits for the heartbeat to suspect and
        fail it, restarts it, replays the AOF, and verifies the data survived.
      </div>
      <button className="btn btn-danger" disabled={busy} onClick={() => void run()}>
        {busy ? "Running…" : "Run Failure Experiment"}
      </button>
      {busy && <div className="panel-hint">Warmup → kill → detect → restart → verify. Watch the timeline below.</div>}
      {error && <ErrorBox message={error} />}
      {result && w && (
        <div className="exp-result">
          <div className="load-run-stats">
            <div className="stat"><span>Detection</span><b className="mono">{result.detection_s.toFixed(1)}s</b></div>
            <div className="stat"><span>Failed requests</span><b className="mono bad">{w.failed_request_pct.toFixed(1)}%</b></div>
            <div className="stat"><span>Restart</span><b className="mono">{result.restart_s.toFixed(2)}s</b></div>
            <div className="stat"><span>AOF recovery</span><b className="mono">{result.aof_recovery_ms}ms</b></div>
            <div className="stat"><span>Healthy again</span><b className="mono">{result.healthy_s.toFixed(1)}s</b></div>
            <div className="stat"><span>Data verified</span><b className="mono">{result.verify_hit_rate.toFixed(1)}%</b></div>
          </div>
          <div className="timeline-body">
            {result.timeline.map((entry, i) => (
              <div key={i} className="timeline-row">
                <span className="timeline-icon">•</span>
                <span className="timeline-label">{entry.label}</span>
                <span className="timeline-time mono">{entry.t_s.toFixed(1)}s</span>
              </div>
            ))}
          </div>
          <div className="load-run-stats">
            <div className="stat"><span>Live GETs</span><b className="mono">{w.gets.toLocaleString()}</b></div>
            <div className="stat"><span>Hits</span><b className="mono">{w.hits.toLocaleString()}</b></div>
            <div className="stat"><span>Errors</span><b className="mono bad">{w.errors.toLocaleString()}</b></div>
          </div>
          <div className="load-run-stats">
            <div className="stat"><span>Outage GETs</span><b className="mono">{w.outage_gets.toLocaleString()}</b></div>
            <div className="stat"><span>Outage errors</span><b className="mono bad">{w.outage_errors.toLocaleString()}</b></div>
          </div>
          <Finding text="AOF replay restores all data after an unplanned node death; reads routed to the dead node fail until it is restarted." />
        </div>
      )}
    </div>
  );
}

function HashingCard() {
  const [keys, setKeys] = useState(100_000);
  const [vnodes, setVnodes] = useState(100);
  const [seed, setSeed] = useState(42);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<HashingResult | null>(null);

  const run = async () => {
    setBusy(true);
    setError(null);
    try {
      setResult(await runHashExperiment({ keys, vnodes, seed }));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const reduction =
    result && result.modulo_moved_pct > 0
      ? ((result.modulo_moved_pct - result.consistent_moved_pct) / result.modulo_moved_pct) * 100
      : null;

  return (
    <div className="exp-card exp-wide">
      <div className="exp-title">Hashing</div>
      <div className="exp-blurb">
        Modulo vs consistent hashing: how many keys move when the cluster grows 3 → 4 nodes. Computed
        in-process with the same routers the client uses.
      </div>
      <div className="field-row">
        <label className="field">
          <span className="field-label">Keys</span>
          <input className="mono field-input" type="number" value={keys} min={1000} onChange={(e) => setKeys(Number(e.target.value))} />
        </label>
        <label className="field">
          <span className="field-label">Vnodes</span>
          <input className="mono field-input" type="number" value={vnodes} min={1} onChange={(e) => setVnodes(Number(e.target.value))} />
        </label>
        <label className="field">
          <span className="field-label">Seed</span>
          <input className="mono field-input" type="number" value={seed} onChange={(e) => setSeed(Number(e.target.value))} />
        </label>
        <button className="btn btn-primary" disabled={busy} onClick={() => void run()}>
          {busy ? "Running…" : "Run Test"}
        </button>
      </div>
      {error && <ErrorBox message={error} />}
      {result && (
        <div className="exp-result">
          <div className="ratio-list">
            <RatioBar pct={result.modulo_moved_pct} color="#EF4444" label="Modulo" />
            <RatioBar pct={result.consistent_moved_pct} color="#22C55E" label="Consistent" />
          </div>
          {reduction !== null && (
            <Finding
              text={`consistent hashing moved approximately ${reduction.toFixed(0)}% fewer keys during node addition (3 → 4 nodes, ${result.keys.toLocaleString()} keys, vnodes ${result.vnodes}) in ${result.elapsed_ms}ms.`}
            />
          )}
        </div>
      )}
    </div>
  );
}

function LoadCard({ onStartLoad, onJumpToLoad }: { onStartLoad: (params: LoadParams) => void; onJumpToLoad: () => void }) {
  const [busy, setBusy] = useState(false);
  const run = async () => {
    setBusy(true);
    try {
      onStartLoad({ clients: 16, requests: 200_000, get_ratio: 0.9, keys: 20_000, value_size: 128, key_order: "uniform", ttl: 0, seed: 42, vnodes: 100, router: "consistent" });
      onJumpToLoad();
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="exp-card">
      <div className="exp-title">Load</div>
      <div className="exp-blurb">Increase client concurrency and watch throughput and latency move.</div>
      <button className="btn btn-primary" disabled={busy} onClick={() => void run()}>
        {busy ? "Starting…" : "Run Test"}
      </button>
      <div className="panel-hint">Opens the Load panel with a 16-client / 200k workload.</div>
    </div>
  );
}

export function Experiments({
  onStartLoad,
  onJumpToLoad,
}: {
  onStartLoad: (params: LoadParams) => void;
  onJumpToLoad: () => void;
}) {
  return (
    <div className="experiments-grid">
      <HashingCard />
      <LoadCard onStartLoad={onStartLoad} onJumpToLoad={onJumpToLoad} />
      <EvictionCard />
      <TtlCard />
      <ReplicationCard />
      <ScalabilityCard />
      <FailureCard />
    </div>
  );
}