

import { useEffect, useRef, useState } from "react";
import { aggregate, pollNodes, type NodeState } from "./api";
import { MAX_SAMPLES, REFRESH_MS } from "./config";
import type { Sample } from "./components/Charts";

function nowLabel(): string {
  return new Date().toLocaleTimeString("en-GB", { hour12: false });
}

export function useMetricsPolling(urls: string[], paused: boolean) {
  const [nodes, setNodes] = useState<NodeState[]>([]);
  const [samples, setSamples] = useState<Sample[]>([]);
  const [error, setError] = useState<string | null>(null);
  const samplesRef = useRef<Sample[]>([]);

  useEffect(() => {
    if (paused) return;
    let cancelled = false;

    const tick = async () => {
      const states = await pollNodes(urls);
      if (cancelled) return;
      setNodes(states);
      const agg = aggregate(states);
      const sample: Sample = {
        t: nowLabel(),
        reqPerSec: agg.reqPerSec,
        hitRate: agg.hitRate,
        p50: agg.p50,
        p95: agg.p95,
        p99: agg.p99,
      };
      const next = [...samplesRef.current, sample];
      if (next.length > MAX_SAMPLES) next.shift();
      samplesRef.current = next;
      setSamples(next);
      setError(agg.up.length === 0 ? "No nodes reachable" : null);
    };

    let id: number | undefined;
    const run = async () => {
      await tick();
      id = window.setInterval(tick, REFRESH_MS);
    };
    void run();

    return () => {
      cancelled = true;
      if (id !== undefined) window.clearInterval(id);
    };
  }, [urls.join(","), paused]);

  return { nodes, samples, error };
}