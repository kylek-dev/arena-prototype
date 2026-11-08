import { createStore } from "../state/store";
import { FrameTiming } from "../domain/frame-timing";
export const debugEnabled = new URLSearchParams(location.search).has("debug");
export const diagnosticsVisible = createStore(debugEnabled);
const started = performance.now();
const timing = new FrameTiming();
const engineTiming = new FrameTiming();
const summary = (now: number) => ({ ...timing.summary(now), engine: engineTiming.summary(now) });
export const frameTiming = createStore(summary(0));
let lastEngineTime: number | null = null;
let lastEngineFrame: number | null = null;
let firstStateMs: number | null = null;
const errors: string[] = [];
export function recordState(frame?: number) {
  const now = performance.now();
  firstStateMs ??= now - started;
  if (frame === undefined || !Number.isSafeInteger(frame) || frame <= 30 || document.hidden) {
    lastEngineTime = lastEngineFrame = null;
    return;
  }
  if (frame === lastEngineFrame) return;
  if (lastEngineTime !== null && lastEngineFrame !== null && frame > lastEngineFrame)
    engineTiming.add(now, now - lastEngineTime);
  lastEngineTime = now;
  lastEngineFrame = frame;
}
export function recordError(message: string) {
  errors.push(message);
}
export function metrics() {
  const { graph: _graph, engine, ...browser } = summary(performance.now());
  const { graph: _engineGraph, ...engineStats } = engine;
  return {
    firstStateMs,
    ...browser,
    engine: engineStats,
    errors: [...errors],
    renderer: "Bevy / WebGL2",
  };
}
export function resetFrameTiming() {
  timing.reset();
  engineTiming.reset();
  lastEngineTime = lastEngineFrame = null;
  frameTiming.set(summary(performance.now()));
}
export function measureFrames(ready: () => boolean) {
  let last: number | null = null;
  let published = 0;
  let request = 0;
  const resetBaseline = () => {
    last = null;
    lastEngineTime = lastEngineFrame = null;
  };
  document.addEventListener("visibilitychange", resetBaseline);
  function frame(now: number) {
    const active = ready() && !document.hidden;
    if (active && last !== null) timing.add(now, now - last);
    last = active ? now : null;
    if (debugEnabled && diagnosticsVisible.get() && now - published >= 250) {
      frameTiming.set(summary(now));
      published = now;
    }
    request = requestAnimationFrame(frame);
  }
  request = requestAnimationFrame(frame);
  return () => {
    cancelAnimationFrame(request);
    document.removeEventListener("visibilitychange", resetBaseline);
  };
}
