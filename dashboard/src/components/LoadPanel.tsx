

import { useState } from "react";
import { DEFAULT_LOAD, type LoadParams, type LoadStatus } from "../control";
import { formatRate, formatUs } from "./StatCard";

export interface LoadEntry {
  id: string;
  params: LoadParams;
  status: LoadStatus | null;
  running: boolean;
}

interface Preset {
  name: string;
  clients: number;
  requests: number;
}

const PRESETS: Preset[] = [
  { name: "Quick 20k", clients: 8, requests: 20_000 },
  { name: "Standard 100k", clients: 8, requests: 100_000 },
  { name: "Heavy 500k", clients: 32, requests: 500_000 },
];

function Field({
  label,
  value,
  onChange,
  step,
  min,
}: {
  label: string;
  value: number | string;
  onChange: (v: number | string) => void;
  step?: number;
  min?: number;
}) {
  return (
    <label className="field">
      <span className="field-label">{label}</span>
      <input
        type="number"
        className="mono field-input"
        value={value}
        min={min}
        step={step}
        onChange={(e) => onChange(e.target.value === "" ? 0 : Number(e.target.value))}
      />
    </label>
  );
}

function Select({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: string;
  options: string[];
  onChange: (v: string) => void;
}) {
  return (
    <label className="field">
      <span className="field-label">{label}</span>
      <select className="mono field-input" value={value} onChange={(e) => onChange(e.target.value)}>
        {options.map((o) => (
          <option key={o} value={o}>
            {o}
          </option>
        ))}
      </select>
    </label>
  );
}

export function LoadPanel({
  entries,
  onStart,
  busy,
}: {
  entries: LoadEntry[];
  onStart: (params: LoadParams) => void;
  busy: boolean;
}) {
  const [form, setForm] = useState<LoadParams>({ ...DEFAULT_LOAD });
  const [error, setError] = useState<string | null>(null);

  const start = () => {
    if (form.clients < 1 || form.requests < 1) {
      setError("clients and requests must be >= 1");
      return;
    }
    setError(null);
    onStart({ ...form });
  };

  const set = (patch: Partial<LoadParams>) => setForm((f) => ({ ...f, ...patch }));

  return (
    <div className="load-panel">
      <section className="panel-section">
        <div className="chart-title">Workload</div>
        <div className="preset-row">
          {PRESETS.map((p) => (
            <button
              key={p.name}
              className="btn btn-sm"
              onClick={() => set({ clients: p.clients, requests: p.requests })}
            >
              {p.name}
            </button>
          ))}
        </div>
        <div className="field-grid">
          <Field label="Clients" value={form.clients} min={1} onChange={(v) => set({ clients: v as number })} />
          <Field label="Requests" value={form.requests} min={1} onChange={(v) => set({ requests: v as number })} />
          <Field
            label="GET ratio"
            value={form.get_ratio}
            min={0}
            step={0.05}
            onChange={(v) => set({ get_ratio: v as number })}
          />
          <Field label="Keyspace" value={form.keys} min={1} onChange={(v) => set({ keys: v as number })} />
          <Field label="Value bytes" value={form.value_size} min={1} onChange={(v) => set({ value_size: v as number })} />
          <Field label="TTL (0=none)" value={form.ttl} min={0} onChange={(v) => set({ ttl: v as number })} />
          <Field label="Seed" value={form.seed} onChange={(v) => set({ seed: v as number })} />
          <Field label="Vnodes" value={form.vnodes} min={1} onChange={(v) => set({ vnodes: v as number })} />
          <Select
            label="Key order"
            value={form.key_order}
            options={["uniform", "sequential"]}
            onChange={(v) => set({ key_order: v })}
          />
          <Select
            label="Router"
            value={form.router}
            options={["consistent", "modulo"]}
            onChange={(v) => set({ router: v })}
          />
        </div>
        {error && <div className="action-error">{error}</div>}
        <div className="btn-row">
          <button className="btn btn-primary" disabled={busy} onClick={start}>
            Start Load
          </button>
        </div>
      </section>

      <section className="panel-section">
        <div className="chart-title">Runs</div>
        {entries.length === 0 ? (
          <div className="panel-hint">No load runs yet.</div>
        ) : (
          <div className="load-runs">
            {entries
              .slice()
              .reverse()
              .map((entry) => (
                <LoadRunCard key={entry.id} entry={entry} />
              ))}
          </div>
        )}
      </section>
    </div>
  );
}

function LoadRunCard({ entry }: { entry: LoadEntry }) {
  const s = entry.status;
  const running = entry.running;
  const report = s?.report ?? null;
  return (
    <div className={`load-run ${running ? "running" : ""}`}>
      <div className="load-run-head">
        <span className="mono load-run-id">{entry.id}</span>
        {running ? (
          <span className="pill pill-accent">running…</span>
        ) : s?.error ? (
          <span className="pill pill-bad">failed</span>
        ) : (
          <span className="pill pill-good">done</span>
        )}
        <span className="load-run-params mono">
          {entry.params.clients}×{entry.params.requests} · {Math.round(entry.params.get_ratio * 100)}% GET ·{" "}
          {entry.params.router}
        </span>
      </div>
      {s?.error && <div className="action-error">{s.error}</div>}
      {report && (
        <div className="load-run-stats">
          <span className="mono stat">{formatRate(report.ops_per_sec)} ops/s</span>
          <span className="mono stat">p50 {formatUs(report.p50_us)}</span>
          <span className="mono stat">p99 {formatUs(report.p99_us)}</span>
          <span className="mono stat">{report.hits.toLocaleString()} hits</span>
          <span className="mono stat">{report.misses.toLocaleString()} misses</span>
          {report.errors > 0 && <span className="mono stat bad">{report.errors} errors</span>}
        </div>
      )}
    </div>
  );
}