import { expect, it } from "vitest";
import { FrameTiming, SPIKE_MS } from "../../web/src/domain/frame-timing";

it("reports empty state, FPS and tail spikes without averaging graph peaks away", () => {
  const timing = new FrameTiming();
  expect(timing.summary(0)).toMatchObject({
    fps: null,
    frameP99Ms: null,
    frameMaxMs: null,
    frameSpikes: 0,
  });
  for (let i = 0; i < 99; i++) timing.add(i * 10, 10);
  timing.add(990, 120);
  const result = timing.summary(999);
  expect(result.frameP50Ms).toBe(10);
  expect(result.frameP95Ms).toBe(10);
  expect(result.frameP99Ms).toBe(10);
  expect(result.frameMaxMs).toBe(120);
  expect(result.frameSpikes).toBe(1);
  expect(result.fps).toBeCloseTo(100_000 / 1110);
  expect(Math.max(...result.graph)).toBe(120);
  expect(timing.summary(7000).graph.every((peak) => peak === 0)).toBe(true);
  expect(timing.summary(7000).fps).toBeNull();
});

it("bounds the rolling history and expires spikes when overwritten or reset", () => {
  const timing = new FrameTiming();
  timing.add(0, 200);
  for (let i = 1; i <= 3600; i++) timing.add(i * 10, 10);
  expect(timing.summary(36_000)).toMatchObject({
    frameSamples: 3600,
    frameMaxMs: 10,
    frameSpikes: 0,
    fps: 100,
  });
  timing.reset();
  timing.add(36_010, 20);
  expect(timing.summary(36_010)).toMatchObject({
    frameSamples: 1,
    frameP99Ms: 20,
    frameMaxMs: 20,
    fps: 50,
  });
});

it("rejects nonfinite/nonpositive intervals and defines a strict spike threshold", () => {
  const timing = new FrameTiming();
  for (const duration of [0, -1, NaN, Infinity]) timing.add(0, duration);
  timing.add(NaN, 10);
  timing.add(10, SPIKE_MS);
  timing.add(20, SPIKE_MS + 0.1);
  expect(timing.summary(20)).toMatchObject({ frameSamples: 2, frameSpikes: 1 });
});
