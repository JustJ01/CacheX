

import {
  Area,
  AreaChart,
  CartesianGrid,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";

export interface Sample {
  t: string;
  reqPerSec: number;
  hitRate: number;
  p50: number;
  p95: number;
  p99: number;
}

const REDUCED = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

const grid = "rgba(148,163,184,0.12)";
const axis = { fill: "#94A3B8", fontSize: 11 };

function tooltipStyle() {
  return {
    contentStyle: {
      background: "#12182B",
      border: "1px solid #1E2A4A",
      borderRadius: 8,
      fontSize: 12,
      fontFamily: "'Fira Code', monospace",
    },
    labelStyle: { color: "#E2E8F0" },
  };
}

export function ThroughputChart({ samples }: { samples: Sample[] }) {
  return (
    <div className="chart-wrap">
      <div className="chart-title">Throughput (req/s)</div>
      <ResponsiveContainer width="100%" height={180}>
        <AreaChart data={samples} margin={{ top: 8, right: 8, left: 0, bottom: 0 }}>
          <defs>
            <linearGradient id="throughputFill" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="#3B82F6" stopOpacity={0.45} />
              <stop offset="100%" stopColor="#3B82F6" stopOpacity={0.02} />
            </linearGradient>
          </defs>
          <CartesianGrid strokeDasharray="3 3" stroke={grid} />
          <XAxis dataKey="t" stroke={axis.fill} tick={axis} minTickGap={40} />
          <YAxis stroke={axis.fill} tick={axis} width={56} />
          <Tooltip {...tooltipStyle()} />
          <Area
            type="monotone"
            dataKey="reqPerSec"
            stroke="#3B82F6"
            strokeWidth={2}
            fill="url(#throughputFill)"
            isAnimationActive={!REDUCED}
            dot={false}
          />
        </AreaChart>
      </ResponsiveContainer>
    </div>
  );
}

export function HitRateChart({ samples }: { samples: Sample[] }) {
  return (
    <div className="chart-wrap">
      <div className="chart-title">Cache hit ratio</div>
      <ResponsiveContainer width="100%" height={180}>
        <LineChart data={samples} margin={{ top: 8, right: 8, left: 0, bottom: 0 }}>
          <CartesianGrid strokeDasharray="3 3" stroke={grid} />
          <XAxis dataKey="t" stroke={axis.fill} tick={axis} minTickGap={40} />
          <YAxis
            stroke={axis.fill}
            tick={axis}
            width={40}
            domain={[0, 100]}
            tickFormatter={(v: number) => `${v}%`}
          />
          <Tooltip {...tooltipStyle()} formatter={(v: number) => [`${(v * 100).toFixed(1)}%`, "hit rate"]} />
          <Line
            type="monotone"
            dataKey="hitRate"
            stroke="#22C55E"
            strokeWidth={2}
            dot={false}
            isAnimationActive={!REDUCED}
          />
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}

export function LatencyChart({ samples }: { samples: Sample[] }) {
  return (
    <div className="chart-wrap">
      <div className="chart-title">GET latency (p50 / p95 / p99)</div>
      <ResponsiveContainer width="100%" height={180}>
        <LineChart data={samples} margin={{ top: 8, right: 8, left: 0, bottom: 0 }}>
          <CartesianGrid strokeDasharray="3 3" stroke={grid} />
          <XAxis dataKey="t" stroke={axis.fill} tick={axis} minTickGap={40} />
          <YAxis stroke={axis.fill} tick={axis} width={56} tickFormatter={(v: number) => `${v}us`} />
          <Tooltip {...tooltipStyle()} formatter={(v: number) => `${v} us`} />
          <Line type="monotone" dataKey="p50" name="p50" stroke="#94A3B8" strokeWidth={1.5} dot={false} isAnimationActive={!REDUCED} />
          <Line type="monotone" dataKey="p95" name="p95" stroke="#D97706" strokeWidth={1.5} dot={false} isAnimationActive={!REDUCED} />
          <Line type="monotone" dataKey="p99" name="p99" stroke="#EF4444" strokeWidth={2} dot={false} isAnimationActive={!REDUCED} />
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}