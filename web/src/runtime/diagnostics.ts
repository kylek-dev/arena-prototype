import { createStore } from "../state/store";
export const debugEnabled = new URLSearchParams(location.search).has("debug");
export const diagnosticsVisible = createStore(debugEnabled);
const started = performance.now();
const frameMs: number[] = [];
let firstStateMs: number | null = null;
const errors: string[] = [];
export function recordState() {
  firstStateMs ??= performance.now() - started;
}
export function recordError(message: string) {
  errors.push(message);
}
export function metrics() {
  const sorted = [...frameMs].sort((a, b) => a - b);
  return {
    firstStateMs,
    frameSamples: sorted.length,
    frameP50Ms: sorted[Math.floor((sorted.length - 1) * 0.5)] ?? null,
    frameP95Ms: sorted[Math.floor((sorted.length - 1) * 0.95)] ?? null,
    errors: [...errors],
    renderer: "Bevy / WebGL2",
  };
}
export function measureFrames(ready: () => boolean) {
  let last: number | null = null;
  function frame(now: number) {
    if (ready() && !document.hidden && last !== null) {
      frameMs.push(now - last);
      if (frameMs.length > 3600) frameMs.shift();
    }
    last = document.hidden ? null : now;
    requestAnimationFrame(frame);
  }
  requestAnimationFrame(frame);
}
