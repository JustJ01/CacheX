

import { useEffect, useRef, useState } from "react";
import { CONTROL_URL } from "./config";
import { publicPort, type ControlEvent, type ExperimentResult } from "./control";

export type TimelineTone = "ok" | "warn" | "err" | "accent" | "info";

export interface TimelineEvent {
  seq: number;
  kind: string;
  label: string;
  tone: TimelineTone;
  time: string;
}

export const MAX_TIMELINE = 200;

function toneFor(kind: string): TimelineTone {
  switch (kind) {
    case "node_killed":
    case "node_failed":
      return "err";
    case "node_suspected":
    case "load_done":
      return "warn";
    case "node_restarted":
    case "load_started":
    case "experiment_done":
      return "accent";
    case "cluster_stopped":
      return "warn";
    default:
      return "ok";
  }
}

function labelFor(raw: ControlEvent): string {
  const kind = raw.type;
  const addr = typeof raw.address === "string" ? publicPort(raw.address) : undefined;
  const node = addr ? `Node ${addr}` : "Node";
  switch (kind) {
    case "cluster_started":
      return `Cluster started (${raw.node_count ?? "?"} nodes)`;
    case "cluster_stopped":
      return "Cluster stopped";
    case "node_started":
      return `${node} started`;
    case "node_killed":
      return `${node} killed`;
    case "node_suspected":
      return `${node} suspected`;
    case "node_failed":
      return `${node} failed`;
    case "node_restarted":
      return `${node} restarted`;
    case "node_healthy":
      return `${node} healthy`;
    case "aof_replayed":
      return `${node} AOF replayed: ${raw.ms ?? 0}ms`;
    case "load_started":
      return `Load ${raw.id ?? "?"} started (${raw.clients ?? "?"} clients, ${raw.requests ?? "?"} req)`;
    case "load_done": {
      const report = raw.report as { ops_per_sec?: number } | undefined;
      const error = raw.error as string | undefined;
      if (error) return `Load ${raw.id ?? "?"} failed: ${error}`;
      return `Load ${raw.id ?? "?"} done — ${Math.round(report?.ops_per_sec ?? 0).toLocaleString()} ops/s`;
    }
    case "experiment_phase":
      return `Experiment ${raw.name ?? "?"}: ${raw.detail ?? ""}`;
    case "experiment_done":
      return `Experiment ${raw.name ?? "?"} complete`;
    case "info":
      return String(raw.message ?? "");
    default:
      return kind;
  }
}

export function useControlEvents(
  onRaw?: (event: ControlEvent) => void,
  enabled = true,
): { timeline: TimelineEvent[]; latestResult: ExperimentResult | null; clearTimeline: () => void } {
  const [timeline, setTimeline] = useState<TimelineEvent[]>([]);
  const [latestResult, setLatestResult] = useState<ExperimentResult | null>(null);
  const seq = useRef(0);
  const onRawRef = useRef(onRaw);
  onRawRef.current = onRaw;

  useEffect(() => {
    if (!enabled) return;
    let closed = false;
    const source = new EventSource(`${CONTROL_URL}/control/events`);

    source.onmessage = (message) => {
      if (closed) return;
      let raw: ControlEvent;
      try {
        raw = JSON.parse(message.data) as ControlEvent;
      } catch {
        return;
      }
      onRawRef.current?.(raw);
      setTimeline((prev) => {
        const next = [
          ...prev,
          {
            seq: seq.current++,
            kind: raw.type,
            label: labelFor(raw),
            tone: toneFor(raw.type),
            time: new Date().toLocaleTimeString("en-GB", { hour12: false }),
          },
        ];
        return next.length > MAX_TIMELINE ? next.slice(next.length - MAX_TIMELINE) : next;
      });
      if (raw.type === "experiment_done") {
        setLatestResult(raw as unknown as ExperimentResult);
      }
    };

    return () => {
      closed = true;
      source.close();
    };
  }, [enabled]);

  return {
    timeline,
    latestResult,
    clearTimeline: () => setTimeline([]),
  };
}