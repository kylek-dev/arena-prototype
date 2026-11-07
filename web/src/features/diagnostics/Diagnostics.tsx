import { engine } from "../../state/session";
import { useStore } from "../../state/store";
import { debugEnabled, diagnosticsVisible, metrics } from "../../runtime/diagnostics";

export function Diagnostics() {
  const state = useStore(engine, (s) => s);
  const visible = useStore(diagnosticsVisible, (s) => s);
  if (!debugEnabled || !visible) return null;
  return (
    <aside id="debug" className="diagnostics" aria-label="Development diagnostics">
      <span className="eyebrow">READ-ONLY ENGINE SNAPSHOT</span>
      <pre id="debug-state">{JSON.stringify({ state, metrics: metrics() }, null, 2)}</pre>
      <p>Frame intervals are not GPU benchmarks. Local simulation; no network authority.</p>
    </aside>
  );
}
