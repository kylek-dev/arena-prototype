import { adaptSnapshot, needsImmediateDelivery } from "../bridge/snapshot";
import { loadEngine } from "../bridge/loader";
import { engine, error, focus } from "../state/session";
import { reconcileFocus } from "../domain/units/selection";
import { installPointer } from "./pointer";
import { installInput } from "./input-router";
import { installSpellDragging } from "./spell-drag";
import { debugEnabled, measureFrames, metrics, recordError, recordState } from "./diagnostics";
export function startRuntime(canvas: HTMLCanvasElement) {
  const pointer = installPointer(canvas, () => router.cancel());
  const router = installInput(canvas, pointer.end);
  const stopSpellDrop = installSpellDragging(canvas);
  const fail = (message: string) => {
    if (error.get()) return;
    recordError(message);
    error.set(
      `${message} Try a current desktop browser with WebGL2 enabled. See the console for details.`,
    );
    router.cancel();
  };
  window.addEventListener("error", (e) => fail(e.message));
  window.addEventListener("unhandledrejection", (e) => fail(String(e.reason?.message ?? e.reason)));
  let lastDelivery = 0;
  window.arenaShell = {
    readInput: router.sample,
    onState(raw) {
      try {
        const next = adaptSnapshot(raw),
          previous = engine.get();
        recordState();
        focus.set(
          reconcileFocus(
            focus.get(),
            [next.player, next.target],
            !!previous && next.tick < previous.tick,
          ),
        );
        // Deliver cooldown edges immediately so starting/finishing never waits
        // for the ordinary HUD throttle. In-between snapshots animate in rAF.
        if (performance.now() - lastDelivery < 80 && !needsImmediateDelivery(previous, next))
          return;
        lastDelivery = performance.now();
        engine.set(next);
        if (!previous && document.activeElement === document.body) canvas.focus();
      } catch (e) {
        fail(String(e));
      }
    },
  };
  if (debugEnabled)
    window.arenaDebug = {
      get state() {
        return structuredClone(engine.get());
      },
      get metrics() {
        return metrics();
      },
    };
  measureFrames(() => (engine.get()?.frames ?? 0) > 30);
  loadEngine().catch((e) => {
    console.error(e);
    fail(String(e.message ?? e));
  });
  return () => {
    router.dispose();
    pointer.dispose();
    stopSpellDrop();
  };
}
