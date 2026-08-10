

function defaultMetricsUrls(): string[] {
  return [
    "http://127.0.0.1:9001/metrics",
    "http://127.0.0.1:9002/metrics",
    "http://127.0.0.1:9003/metrics",
  ];
}

const metricsEnv = import.meta.env.VITE_METRICS_URLS as string | undefined;

export const METRICS_URLS: string[] = metricsEnv
  ? metricsEnv.split(",").map((s: string) => s.trim()).filter(Boolean)
  : defaultMetricsUrls();

export const REFRESH_MS = 2000;
export const MAX_SAMPLES = 120;

export const CONTROL_URL: string = (import.meta.env.VITE_CONTROL_URL as string | undefined)
  ? import.meta.env.VITE_CONTROL_URL.replace(/\/$/, "")
  : "http://127.0.0.1:9100";

export function nodeLabel(url: string): string {
  const match = url.match(/^https?:\/\/([^/]+)/);
  return match ? match[1] : url;
}