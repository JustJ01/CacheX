

import { useEffect, useMemo, useState } from "react";
import { aggregate, type NodeState } from "./api";
import { METRICS_URLS } from "./config";
import {
  getClusterStatus,
  getHealth,
  getLoad,
  runHashExperiment,
  startLoad as apiStartLoad,
  type ClusterStatus,
  type Health,
  type LoadParams,
  type Report,
} from "./control";
import { useControlEvents } from "./useControlEvents";
import { useMetricsPolling } from "./useMetricsPolling";
import { ClusterControl } from "./components/ClusterControl";
import { Experiments } from "./components/Experiments";
import { LoadPanel, type LoadEntry } from "./components/LoadPanel";
import { Overview } from "./components/Overview";
import { Results } from "./components/Results";
import { Timeline } from "./components/Timeline";

type Tab = "overview" | "control" | "experiments" | "results";

const TABS: { id: Tab; label: string }[] = [
  { id: "overview", label: "Overview" },
  { id: "control", label: "Cluster Control" },
  { id: "experiments", label: "Experiments" },
  { id: "results", label: "Results" },
];

export default function App() {
  const [tab, setTab] = useState<Tab>("overview");
  const [status, setStatus] = useState<ClusterStatus | null>(null);
  const [health, setHealth] = useState<Health | null>(null);
  const [controlDown, setControlDown] = useState(false);
  const [paused, setPaused] = useState(false);
  const [loadEntries, setLoadEntries] = useState<Record<string, LoadEntry>>({});
  const [actionError, setActionError] = useState<string | null>(null);

  const metricsUrls = useMemo(
    () =>
      status
        ? status.nodes.map((n) => `http://${status.spec.host}:${n.metrics_port}/metrics`)
        : METRICS_URLS,
    [status],
  );

  const { nodes, samples, error } = useMetricsPolling(metricsUrls, paused);

  
  const { timeline, latestResult, clearTimeline } = useControlEvents((event) => {
    if (event.type === "load_done") {
      const id = String(event.id ?? "");
      if (!id) return;
      setLoadEntries((prev) => {
        const entry = prev[id];
        if (!entry) return prev;
        return {
          ...prev,
          [id]: {
            ...entry,
            running: false,
            status: {
              id,
              clients: entry.params.clients,
              requests: entry.params.requests,
              get_ratio: entry.params.get_ratio,
              keys: entry.params.keys,
              value_size: entry.params.value_size,
              done: true,
              report: (event.report as Report | undefined) ?? null,
              error: (event.error as string | undefined) ?? null,
            },
          },
        };
      });
    }
  });

  
  useEffect(() => {
    let cancelled = false;
    const refresh = async () => {
      try {
        const [h, s] = await Promise.all([getHealth(), getClusterStatus()]);
        if (cancelled) return;
        setHealth(h);
        setStatus(s);
        setControlDown(false);
      } catch {
        if (!cancelled) setControlDown(true);
      }
    };
    void refresh();
    const id = window.setInterval(refresh, 3000);
    return () => {
      cancelled = true;
      window.clearInterval(id);
    };
  }, []);

  
  const runningIds = Object.values(loadEntries)
    .filter((e) => e.running)
    .map((e) => e.id)
    .join(",");
  useEffect(() => {
    if (!runningIds) return;
    let cancelled = false;
    const refresh = async () => {
      for (const id of runningIds.split(",")) {
        try {
          const s = await getLoad(id);
          if (cancelled) return;
          setLoadEntries((prev) =>
            prev[id] ? { ...prev, [id]: { ...prev[id], running: !s.done, status: s } } : prev,
          );
        } catch {
          
        }
      }
    };
    void refresh();
    const id = window.setInterval(refresh, 2000);
    return () => {
      cancelled = true;
      window.clearInterval(id);
    };
  }, [runningIds]);

  const agg = aggregate(nodes);
  const total = status?.spec.node_count ?? 3;

  const onStartLoad = async (params: LoadParams) => {
    setActionError(null);
    try {
      const { id } = await apiStartLoad(params);
      setLoadEntries((prev) => ({
        ...prev,
        [id]: { id, params, status: null, running: true },
      }));
    } catch (e) {
      setActionError(e instanceof Error ? e.message : String(e));
    }
  };

  const onRunHash = async () => {
    setActionError(null);
    try {
      await runHashExperiment({ keys: 100_000, vnodes: 100, seed: 42 });
    } catch (e) {
      setActionError(e instanceof Error ? e.message : String(e));
    }
  };

  const loadEntriesList = useMemo(() => Object.values(loadEntries), [loadEntries]);

  return (
    <div className="app">
      <header className="app-header">
        <div>
          <h1>CacheX</h1>
          <span className="tagline">distributed cache control center</span>
        </div>
        <div className="header-right">
          {actionError && <span className="error-banner">{actionError}</span>}
          {error && <span className="error-banner">{error}</span>}
          <span className={`pill ${controlDown ? "pill-bad" : "pill-good"}`}>
            {controlDown ? "control offline" : "control connected"}
          </span>
          <span className={`pill ${agg.up.length === total ? "pill-good" : "pill-bad"}`}>
            {agg.up.length}/{total} healthy
          </span>
          <span className="refresh-note">{paused ? "paused" : "live"}</span>
          <button className="btn" onClick={() => setPaused((p) => !p)}>
            {paused ? "Resume" : "Pause"}
          </button>
        </div>
      </header>

      <nav className="tab-nav">
        {TABS.map((t) => (
          <button
            key={t.id}
            className={`tab-btn ${tab === t.id ? "active" : ""}`}
            onClick={() => setTab(t.id)}
          >
            {t.label}
          </button>
        ))}
      </nav>

      {tab === "overview" && <Overview status={status} nodes={nodes} samples={samples} />}

      {tab === "control" && (
        <div className="control-page">
          <ClusterControl
            status={status}
            nodes={nodes}
            onClusterStatus={setStatus}
            disabled={controlDown}
          />
          <LoadPanel entries={loadEntriesList} onStart={onStartLoad} busy={controlDown} />
        </div>
      )}

      {tab === "experiments" && (
        <Experiments
          onStartLoad={(params) => void onStartLoad(params)}
          onJumpToLoad={() => setTab("control")}
        />
      )}

      {tab === "results" && (
        <Results latestResult={latestResult} loads={loadEntriesList} onRunHash={() => void onRunHash()} />
      )}

      <section className="panel-section timeline-section">
        <Timeline events={timeline} onClear={clearTimeline} />
      </section>

      <footer className="app-footer">
        CacheX — metrics polled live from node /metrics; cluster actions via cachex-control SSE
        {health ? ` · control ${health.ok ? "ok" : "degraded"}` : ""}
      </footer>
    </div>
  );
}

export type { NodeState };