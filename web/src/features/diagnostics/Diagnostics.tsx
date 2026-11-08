import { useState } from "react";
import { engine } from "../../state/session";
import { useStore } from "../../state/store";
import {
  debugEnabled,
  diagnosticsVisible,
  frameTiming,
  metrics,
  resetFrameTiming,
} from "../../runtime/diagnostics";

const milliseconds = (value: number | null) => (value === null ? "—" : `${value.toFixed(1)} ms`);

function SnapshotDetails() {
  const state = useStore(engine, (s) => s);
  return <pre id="debug-state">{JSON.stringify({ state, metrics: metrics() }, null, 2)}</pre>;
}

function PerformancePanel() {
  const stats = useStore(frameTiming, (s) => s);
  const [details, setDetails] = useState(false);
  const y = (ms: number) => 64 - Math.min(100, ms) * 0.64;
  const points = stats.graph
    .map((ms, i) => `${(i * 240) / (stats.graph.length - 1)},${y(ms)}`)
    .join(" ");
  const enginePoints = stats.engine.graph
    .map((ms, i) => `${(i * 240) / (stats.engine.graph.length - 1)},${y(ms)}`)
    .join(" ");
  return (
    <aside id="debug" className="diagnostics" aria-label="Frame timing diagnostics">
      <div className="diagnostics-heading">
        <strong>{stats.fps === null ? "—" : Math.round(stats.fps)} browser FPS</strong>
        <button type="button" onClick={resetFrameTiming}>
          Reset samples
        </button>
      </div>
      <div className="engine-frame-rate">
        <strong>{stats.engine.fps === null ? "—" : Math.round(stats.engine.fps)} engine Hz</strong>
        <span>
          {" "}
          P99 {milliseconds(stats.engine.frameP99Ms)} · Max {milliseconds(stats.engine.frameMaxMs)}
        </span>
      </div>
      <dl className="frame-statistics">
        <div>
          <dt>P50</dt>
          <dd>{milliseconds(stats.frameP50Ms)}</dd>
        </div>
        <div>
          <dt>P95</dt>
          <dd>{milliseconds(stats.frameP95Ms)}</dd>
        </div>
        <div>
          <dt>P99</dt>
          <dd>{milliseconds(stats.frameP99Ms)}</dd>
        </div>
        <div>
          <dt>Max</dt>
          <dd>{milliseconds(stats.frameMaxMs)}</dd>
        </div>
      </dl>
      <div className="frame-spikes">
        Spikes &gt;33.3 ms: <strong>{stats.frameSpikes}</strong> · {stats.frameSamples}/3600 samples
      </div>
      <svg
        className="frame-graph"
        viewBox="0 0 240 64"
        role="img"
        aria-label="Frame-time peaks over the last six seconds; 100 millisecond scale"
      >
        <title>
          Peak frame time per 50 ms bucket; dotted lines at 16.7 and 33.3 ms. Peaks above 100 ms are
          clipped.
        </title>
        <line x1="0" x2="240" y1={y(1000 / 60)} y2={y(1000 / 60)} />
        <line x1="0" x2="240" y1={y(1000 / 30)} y2={y(1000 / 30)} />
        <polyline points={points} />
        <polyline className="engine-frame-line" points={enginePoints} />
      </svg>
      <p>Last 6 s · peak/50 ms · guides 16.7/33.3 ms</p>
      <p>Gold: browser · Cyan: engine updates (not GPU presentation).</p>
      <p>Rates: last 1 s. Statistics: last 3600 intervals per series.</p>
      <button
        type="button"
        aria-expanded={details}
        aria-controls="debug-state"
        onClick={() => setDetails(!details)}
      >
        {details ? "Hide" : "Show"} engine snapshot
      </button>
      {details && <SnapshotDetails />}
    </aside>
  );
}

export function Diagnostics() {
  const visible = useStore(diagnosticsVisible, (s) => s);
  if (!debugEnabled || !visible) return null;
  return <PerformancePanel />;
}
