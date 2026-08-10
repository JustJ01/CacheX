

import { CONTROL_URL } from "./config";

export interface ClusterSpec {
  host: string;
  public_base: number;
  metrics_base: number;
  node_count: number;
  replication_factor: number;
  vnodes: number;
  max_memory_bytes: number;
}

export interface NodeInfo {
  id: number;
  public_port: number;
  metrics_port: number;
  address: string;
  pid: number | null;
}

export interface ClusterStatus {
  spec: ClusterSpec;
  nodes: NodeInfo[];
  ready: boolean;
}

export interface Health {
  ok: boolean;
  root: string;
  server_ready: boolean;
  control_dir: string;
}

export interface LoadParams {
  clients: number;
  requests: number;
  get_ratio: number;
  keys: number;
  value_size: number;
  key_order: string;
  ttl: number;
  seed: number;
  vnodes: number;
  router: string;
}

export const DEFAULT_LOAD: LoadParams = {
  clients: 8,
  requests: 50_000,
  get_ratio: 0.9,
  keys: 20_000,
  value_size: 128,
  key_order: "uniform",
  ttl: 0,
  seed: 42,
  vnodes: 100,
  router: "consistent",
};

export interface Report {
  nodes: number;
  router: string;
  vnodes: number;
  clients: number;
  requests: number;
  keys: number;
  value_size: number;
  get_ratio: number;
  seed: number;
  key_order: string;
  total_secs: number;
  ops_per_sec: number;
  gets: number;
  sets: number;
  hits: number;
  misses: number;
  errors: number;
  avg_us: number;
  p50_us: number;
  p95_us: number;
  p99_us: number;
  max_us: number;
}

export interface LoadStatus {
  id: string;
  clients: number;
  requests: number;
  get_ratio: number;
  keys: number;
  value_size: number;
  done: boolean;
  report: Report | null;
  error: string | null;
}

export interface LiveLoadSample {
  node: string;
  req_per_s: number;
}

export interface HashingResult {
  keys: number;
  vnodes: number;
  seed: number;
  elapsed_ms: number;
  modulo_moved: number;
  consistent_moved: number;
  modulo_moved_pct: number;
  consistent_moved_pct: number;
  live_load: LiveLoadSample[];
}

export interface ExperimentResult {
  status?: string;
  message?: string;
  [key: string]: unknown;
}

export interface EvictionCurvePoint {
  keys: number;
  resident: number;
  hit_rate: number;
}

export interface EvictionResult {
  status: string;
  capacity_bytes: number;
  capacity_mb: number;
  working_set: number;
  value_size: number;
  resident_keys: number;
  evictions: number;
  evicted_pct: number;
  hit_rate: number;
  curve: EvictionCurvePoint[];
  elapsed_ms: number;
}

export interface TtlSample {
  elapsed_secs: number;
  remaining: number;
  hit_rate: number;
}

export interface TtlResult {
  status: string;
  keys: number;
  ttl_secs: number;
  value_size: number;
  samples: TtlSample[];
  duration_secs: number;
  total_expired: number;
  elapsed_ms: number;
}

export interface ReplicationRow {
  rf: number;
  copies: number;
  report: Report;
  used_bytes: number;
  keys: number;
  repl_sent: number;
  repl_received: number;
}

export interface ReplicationComparison {
  throughput_rf1: number;
  throughput_rf2: number;
  throughput_change_pct: number;
  memory_rf1: number;
  memory_rf2: number;
  memory_change_pct: number;
  replication_sent: number;
}

export interface ReplicationResult {
  status: string;
  factors: number[];
  rows: ReplicationRow[];
  comparison?: ReplicationComparison;
  finding: string;
  elapsed_ms: number;
}

export interface ScalabilityPoint {
  clients: number;
  ops_per_sec: number;
  p99_us: number;
  errors: number;
  total_secs: number;
}

export interface ScalabilityResult {
  status: string;
  clients: number[];
  points: ScalabilityPoint[];
  peak: ScalabilityPoint;
  rollover: boolean;
  finding: string;
  elapsed_ms: number;
}

export interface FailureTimelineEntry {
  t_s: number;
  label: string;
}

export interface FailureWorkloadStats {
  gets: number;
  hits: number;
  misses: number;
  errors: number;
  outage_gets: number;
  outage_errors: number;
  failed_request_pct: number;
}

export interface FailureResult {
  status: string;
  node_id: number;
  public_port: number;
  detection_s: number;
  suspicion_s: number;
  failure_s: number;
  restart_s: number;
  aof_recovery_ms: number;
  healthy_s: number;
  verify_hit_rate: number;
  workload: FailureWorkloadStats;
  timeline: FailureTimelineEntry[];
  finding: string;
  elapsed_ms: number;
}

export interface ControlEvent {
  type: string;
  [key: string]: unknown;
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`${CONTROL_URL}${path}`, {
    ...init,
    headers: { "Content-Type": "application/json", ...(init?.headers ?? {}) },
    signal: init?.signal ?? AbortSignal.timeout(15_000),
  });
  if (!response.ok) {
    const body = await response.text().catch(() => "");
    throw new Error(`HTTP ${response.status} ${response.statusText}${body ? `: ${body.slice(0, 200)}` : ""}`);
  }
  return (await response.json()) as T;
}

function post<T>(path: string, body?: unknown): Promise<T> {
  return request<T>(path, { method: "POST", body: body === undefined ? undefined : JSON.stringify(body) });
}

export function getHealth(): Promise<Health> {
  return request<Health>("/control/health");
}

export function getClusterStatus(): Promise<ClusterStatus> {
  return request<ClusterStatus>("/control/cluster/status");
}

export function startCluster(nodes?: number): Promise<ClusterStatus> {
  return post<ClusterStatus>("/control/cluster/start", nodes === undefined ? {} : { nodes });
}

export function stopCluster(): Promise<ClusterStatus> {
  return post<ClusterStatus>("/control/cluster/stop");
}

export function killNode(nodeId: number): Promise<{ node: number; pid: number }> {
  return post<{ node: number; pid: number }>(`/control/cluster/node/${nodeId}/kill`);
}

export function restartNode(nodeId: number): Promise<{ node: number; restarted: boolean }> {
  return post<{ node: number; restarted: boolean }>(`/control/cluster/node/${nodeId}/restart`);
}

export function scaleCluster(target: number): Promise<ClusterStatus> {
  return post<ClusterStatus>("/control/cluster/scale", { target });
}

export function setReplication(factor: number): Promise<ClusterStatus> {
  return post<ClusterStatus>("/control/cluster/replication", { factor });
}

export function startLoad(params: LoadParams): Promise<{ id: string }> {
  return post<{ id: string }>("/control/load/start", params);
}

export function getLoad(id: string): Promise<LoadStatus> {
  return request<LoadStatus>(`/control/load/${id}`);
}

export interface HashExperimentParams {
  keys: number;
  vnodes: number;
  seed: number;
}

export function runHashExperiment(params: HashExperimentParams): Promise<HashingResult> {
  return post<HashingResult>("/control/experiment/hashing", params);
}

export interface EvictionExperimentParams {
  capacity_bytes?: number;
  working_set?: number;
  value_size?: number;
  samples?: number[];
}

export interface TtlExperimentParams {
  keys?: number;
  ttl_secs?: number;
  value_size?: number;
  interval_secs?: number;
}

export interface ReplicationExperimentParams {
  clients?: number;
  requests?: number;
  get_ratio?: number;
  keys?: number;
  value_size?: number;
  factors?: number[];
}

export interface ScalabilityExperimentParams {
  clients?: number[];
  requests?: number;
  get_ratio?: number;
  keys?: number;
  value_size?: number;
}

export interface FailureExperimentParams {
  node_id?: number;
  warmup_sets?: number;
  verify_samples?: number;
  workers?: number;
  value_size?: number;
  vnodes?: number;
}

export function runEvictionExperiment(params?: EvictionExperimentParams): Promise<EvictionResult> {
  return post<EvictionResult>("/control/experiment/eviction", params ?? {});
}

export function runTtlExperiment(params?: TtlExperimentParams): Promise<TtlResult> {
  return post<TtlResult>("/control/experiment/ttl", params ?? {});
}

export function runReplicationExperiment(params?: ReplicationExperimentParams): Promise<ReplicationResult> {
  return post<ReplicationResult>("/control/experiment/replication", params ?? {});
}

export function runScalabilityExperiment(params?: ScalabilityExperimentParams): Promise<ScalabilityResult> {
  return post<ScalabilityResult>("/control/experiment/scalability", params ?? {});
}

export function runFailureExperiment(params?: FailureExperimentParams): Promise<FailureResult> {
  return post<FailureResult>("/control/experiment/failure", params ?? {});
}

export function runStubExperiment(name: string): Promise<ExperimentResult> {
  return post<ExperimentResult>(`/control/experiment/${name}`, {});
}

export function getLatestResult(): Promise<ExperimentResult> {
  return request<ExperimentResult>("/control/results/latest");
}

export function publicPort(address: string): string {
  const parts = address.split(":");
  return parts[parts.length - 1];
}

export function nowLabel(): string {
  return new Date().toLocaleTimeString("en-GB", { hour12: false });
}