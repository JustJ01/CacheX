

import type { ExperimentResult, Report } from "../control";
import type { LoadEntry } from "./LoadPanel";
import { formatUs } from "./StatCard";

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

export function Results({
  latestResult,
  loads,
  onRunHash,
}: {
  latestResult: ExperimentResult | null;
  loads: LoadEntry[];
  onRunHash: () => void;
}) {
  const isHashing =
    latestResult &&
    typeof latestResult.modulo_moved_pct === "number" &&
    typeof latestResult.consistent_moved_pct === "number";
  const isStub = latestResult && typeof latestResult.status === "string";

  const doneLoads = loads.filter((l) => l.status?.report).slice(0, 6);

  return (
    <div className="results-page">
      <section className="panel-section">
        <div className="chart-title">Latest experiment</div>
        {isHashing ? (
          <HashingResultView result={latestResult as unknown as Record<string, number | string>} />
        ) : isStub && latestResult.status === "not_implemented" ? (
          <div className="panel-hint">
            {latestResult.message ?? String(latestResult.status)} — run a Phase 1 experiment (hashing) to see a
            finding here.
          </div>
        ) : isStub ? (
          <div className="panel-hint">
            The <span className="mono">{String(latestResult.name ?? "latest")}</span> experiment completed — its
            full result renders inline on the Experiments tab.
          </div>
        ) : (
          <div className="panel-hint">
            No experiment run yet. Run the hashing experiment from the Experiments tab, or from here.
          </div>
        )}
        <div className="btn-row">
          <button className="btn btn-primary" onClick={onRunHash}>
            Run Hash Test
          </button>
        </div>
      </section>

      <section className="panel-section">
        <div className="chart-title">Benchmark reports</div>
        {doneLoads.length === 0 ? (
          <div className="panel-hint">No completed load runs yet. Start one from the Load panel.</div>
        ) : (
          <div className="report-list">
            {doneLoads.map((l) => (
              <ReportCard key={l.id} id={l.id} report={l.status!.report!} />
            ))}
          </div>
        )}
      </section>
    </div>
  );
}

function HashingResultView({ result }: { result: Record<string, number | string> }) {
  const modulo = Number(result.modulo_moved_pct);
  const consistent = Number(result.consistent_moved_pct);
  const reduction = modulo > 0 ? ((modulo - consistent) / modulo) * 100 : 0;
  return (
    <div className="exp-result">
      <div className="result-head">
        <div className="result-title">Hashing experiment — 3 → 4 nodes</div>
        <div className="result-meta mono">
          {Number(result.keys).toLocaleString()} keys · vnodes {result.vnodes} · seed {result.seed} ·{" "}
          {result.elapsed_ms}ms
        </div>
      </div>
      <div className="ratio-list">
        <RatioBar pct={modulo} color="#EF4444" label="Modulo" />
        <RatioBar pct={consistent} color="#22C55E" label="Consistent Hashing" />
      </div>
      <div className="exp-finding">
        <strong>Finding:</strong> consistent hashing moved approximately {reduction.toFixed(0)}% fewer keys than
        modulo hashing during node addition — the ring only remaps the arcs that changed instead of nearly every
        key.
      </div>
    </div>
  );
}

function ReportCard({ id, report }: { id: string; report: Report }) {
  const hitRate = report.gets > 0 ? (report.hits / report.gets) * 100 : 0;
  return (
    <div className="report-card">
      <div className="report-card-head">
        <span className="mono">{id}</span>
        <span className="mono muted">
          {report.clients}×{report.requests} · {Math.round(report.get_ratio * 100)}% GET · {report.router} · RF
          sidecar
        </span>
      </div>
      <div className="report-grid">
        <ReportCell label="Throughput" value={`${Math.round(report.ops_per_sec).toLocaleString()} ops/s`} />
        <ReportCell label="Total time" value={`${report.total_secs.toFixed(2)}s`} />
        <ReportCell label="Hit rate" value={`${hitRate.toFixed(1)}%`} />
        <ReportCell label="p50" value={formatUs(report.p50_us)} />
        <ReportCell label="p95" value={formatUs(report.p95_us)} />
        <ReportCell label="p99" value={formatUs(report.p99_us)} />
        <ReportCell label="Gets / Sets" value={`${report.gets.toLocaleString()} / ${report.sets.toLocaleString()}`} />
        <ReportCell label="Hits / Misses" value={`${report.hits.toLocaleString()} / ${report.misses.toLocaleString()}`} />
        <ReportCell label="Errors" value={report.errors.toLocaleString()} />
      </div>
    </div>
  );
}

function ReportCell({ label, value }: { label: string; value: string }) {
  return (
    <div className="report-cell">
      <div className="report-cell-label">{label}</div>
      <div className="mono report-cell-value">{value}</div>
    </div>
  );
}