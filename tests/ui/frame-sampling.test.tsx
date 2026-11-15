import { afterEach, expect, it, vi } from "vitest";

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  history.replaceState(null, "", "/");
});

it("measures engine updates separately from browser frames and ignores duplicate/reset/hidden snapshots", async () => {
  vi.resetModules();
  let now = 0;
  vi.spyOn(performance, "now").mockImplementation(() => now);
  let hidden = false;
  vi.spyOn(document, "hidden", "get").mockImplementation(() => hidden);
  const diagnostics = await import("../../web/src/runtime/diagnostics");
  diagnostics.recordState(30);
  now = 100;
  diagnostics.recordState(31);
  now = 116;
  diagnostics.recordState(32);
  now = 120;
  diagnostics.recordState(32);
  now = 132;
  diagnostics.recordState(33);
  expect(diagnostics.metrics()).toMatchObject({
    frameSamples: 0,
    engine: { frameSamples: 2, fps: 62.5, frameMaxMs: 16 },
  });
  now = 1000;
  diagnostics.recordState(0);
  now = 2000;
  diagnostics.recordState(31);
  now = 2016;
  diagnostics.recordState(32);
  expect(diagnostics.metrics().engine.frameMaxMs).toBe(16);
  hidden = true;
  now = 3000;
  diagnostics.recordState(33);
  hidden = false;
  now = 4000;
  diagnostics.recordState(34);
  now = 4004;
  diagnostics.recordState(35);
  expect(diagnostics.metrics().engine).toMatchObject({ frameSamples: 4, fps: 250, frameMaxMs: 16 });
  diagnostics.resetFrameTiming();
  expect(diagnostics.metrics().engine.frameSamples).toBe(0);
});

it("samples only ready visible frames, throttles publishing and disposes its loop", async () => {
  vi.resetModules();
  history.replaceState(null, "", "/?debug=1");
  let next: FrameRequestCallback = () => {};
  vi.stubGlobal(
    "requestAnimationFrame",
    vi.fn((callback: FrameRequestCallback) => {
      next = callback;
      return 42;
    }),
  );
  const cancel = vi.fn();
  vi.stubGlobal("cancelAnimationFrame", cancel);
  let hidden = false;
  vi.spyOn(document, "hidden", "get").mockImplementation(() => hidden);
  const diagnostics = await import("../../web/src/runtime/diagnostics");
  let ready = false;
  const stop = diagnostics.measureFrames(() => ready);
  next(0);
  next(1000);
  expect(diagnostics.metrics().frameSamples).toBe(0);
  ready = true;
  next(2000);
  next(2010);
  expect(diagnostics.metrics().frameSamples).toBe(1);
  next(2260);
  expect(diagnostics.frameTiming.get().frameMaxMs).toBe(250);
  hidden = true;
  document.dispatchEvent(new Event("visibilitychange"));
  next(20_000);
  hidden = false;
  document.dispatchEvent(new Event("visibilitychange"));
  next(30_000);
  next(30_010);
  expect(diagnostics.metrics().frameMaxMs).toBe(250);
  const updates = vi.fn();
  const unsubscribe = diagnostics.frameTiming.subscribe(updates);
  next(30_020);
  next(30_030);
  expect(updates).not.toHaveBeenCalled();
  next(30_300);
  expect(updates).toHaveBeenCalledTimes(1);
  diagnostics.diagnosticsVisible.set(false);
  next(31_000);
  expect(updates).toHaveBeenCalledTimes(1);
  diagnostics.resetFrameTiming();
  expect(diagnostics.metrics().frameSamples).toBe(0);
  stop();
  unsubscribe();
  expect(cancel).toHaveBeenCalledWith(42);
});
