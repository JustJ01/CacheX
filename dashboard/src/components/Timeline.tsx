

import { useEffect, useRef } from "react";
import type { TimelineEvent } from "../useControlEvents";

const ICON: Record<string, string> = {
  ok: "✓",
  warn: "⚠",
  err: "✕",
  accent: "⚡",
  info: "ℹ",
};

export function Timeline({
  events,
  onClear,
}: {
  events: TimelineEvent[];
  onClear?: () => void;
}) {
  const endRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    endRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [events.length]);

  return (
    <div className="timeline">
      <div className="timeline-head">
        <span className="chart-title">Live activity</span>
        <span className="timeline-count mono">
          {events.length} event{events.length === 1 ? "" : "s"}
        </span>
        {onClear && events.length > 0 && (
          <button className="btn btn-sm" onClick={onClear}>
            Clear
          </button>
        )}
      </div>
      <div className="timeline-body">
        {events.length === 0 ? (
          <div className="timeline-empty">Waiting for events…</div>
        ) : (
          events.map((event) => (
            <div key={event.seq} className={`timeline-row tone-${event.tone}`}>
              <span className="timeline-icon">{ICON[event.tone]}</span>
              <span className="timeline-label">{event.label}</span>
              <span className="timeline-time mono">{event.time}</span>
            </div>
          ))
        )}
        <div ref={endRef} />
      </div>
    </div>
  );
}