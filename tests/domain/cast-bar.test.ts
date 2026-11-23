import { describe, expect, it, vi } from "vitest";
import { createCastBarAnimator } from "../../web/src/runtime/cast-bar-animation";
import {
  CAST_RESYNC_SECONDS,
  CAST_STALL_SECONDS,
  CastTimeline,
} from "../../web/src/domain/cast-bar";
import type { ActiveCast } from "../../web/src/domain/casting";

const bolt = (remainingTicks: number): ActiveCast => ({
  spell: "rift_bolt",
  remainingTicks,
  totalTicks: 45,
});

describe("CastTimeline", () => {
  it("is idle without a cast or with an invalid tick rate", () => {
    const timeline = new CastTimeline();
    expect(timeline.visual(0)).toEqual({ active: false, progress: 0, remaining: 0, settled: true });
    timeline.observe({ cast: bolt(45), tickRate: 0 }, 0);
    expect(timeline.visual(0).active).toBe(false);
    timeline.observe({ cast: bolt(45), tickRate: Number.NaN }, 0);
    expect(timeline.visual(0).active).toBe(false);
  });

  it("fills continuously between throttled samples from the cast's start and duration", () => {
    const timeline = new CastTimeline();
    // 1.5 s cast at 30 Hz, observed 5 ticks in.
    timeline.observe({ cast: bolt(40), tickRate: 30 }, 1_000);
    expect(timeline.visual(1_000).progress).toBeCloseTo(5 / 45);
    // Every frame moves forward, not only when a sample arrives.
    const samples = [1_016, 1_033, 1_050, 1_066];
    const progress = samples.map((t) => timeline.visual(t).progress);
    for (let i = 1; i < progress.length; i += 1)
      expect(progress[i]).toBeGreaterThan(progress[i - 1]);
    expect(timeline.visual(1_100).progress).toBeCloseTo((5 + 3) / 45);
    expect(timeline.visual(1_100).remaining).toBeCloseTo(1.5 - 8 / 30);
  });

  it("never steps backwards on small sample jitter but resyncs on real drift", () => {
    const timeline = new CastTimeline();
    timeline.observe({ cast: bolt(45), tickRate: 30 }, 0);
    // A sample slightly behind the running estimate (1 tick) keeps the estimate.
    const before = timeline.visual(100).progress;
    timeline.observe({ cast: bolt(43), tickRate: 30 }, 100);
    expect(timeline.visual(100).progress).toBe(before);
    // A sample far behind (server stalled, then resumed) snaps to it.
    timeline.observe({ cast: bolt(42), tickRate: 30 }, 1_000);
    const drift = 1 - 3 / 30;
    expect(drift).toBeGreaterThan(CAST_RESYNC_SECONDS);
    expect(timeline.visual(1_000).progress).toBeCloseTo(3 / 45);
  });

  it("stops a short way past the last sample when snapshots stall, then settles", () => {
    const timeline = new CastTimeline();
    timeline.observe({ cast: bolt(45), tickRate: 30 }, 0);
    expect(timeline.visual(100).settled).toBe(false);
    // Re-observing the same sample (a re-render) adds no time.
    timeline.observe({ cast: bolt(45), tickRate: 30 }, 5_000);
    const stalled = timeline.visual(5_000);
    expect(stalled.progress).toBeCloseTo(CAST_STALL_SECONDS / 1.5);
    expect(stalled.settled).toBe(true);
    // A fresh sample un-settles it.
    timeline.observe({ cast: bolt(30), tickRate: 30 }, 5_010);
    expect(timeline.visual(5_010).settled).toBe(false);
  });

  it("pulls forward to a sample that is ahead of the bar", () => {
    const timeline = new CastTimeline();
    timeline.observe({ cast: bolt(45), tickRate: 30 }, 0);
    // 50 ms later the server already reports 3 ticks (100 ms) in.
    timeline.observe({ cast: bolt(42), tickRate: 30 }, 50);
    expect(timeline.visual(50).progress).toBeCloseTo(3 / 45);
  });

  it("snaps when a new cast starts, including back-to-back casts of the same spell", () => {
    const timeline = new CastTimeline();
    timeline.observe({ cast: bolt(2), tickRate: 30 }, 0);
    expect(timeline.visual(0).progress).toBeCloseTo(43 / 45);
    timeline.observe({ cast: bolt(45), tickRate: 30 }, 40);
    expect(timeline.visual(40).progress).toBe(0);
    timeline.observe(
      { cast: { spell: "mend", remainingTicks: 30, totalTicks: 45 }, tickRate: 30 },
      50,
    );
    expect(timeline.visual(50).progress).toBeCloseTo(15 / 45);
    timeline.observe(null, 60);
    expect(timeline.visual(60).active).toBe(false);
  });

  it("caps at a full bar until the cast ends", () => {
    const timeline = new CastTimeline();
    timeline.observe({ cast: bolt(1), tickRate: 30 }, 0);
    const end = timeline.visual(10_000);
    expect(end.progress).toBe(1);
    expect(end.remaining).toBe(0);
    expect(end.settled).toBe(true);
  });
});

describe("createCastBarAnimator", () => {
  it("runs one loop while any bar moves, sleeps when all settle and wakes on demand", () => {
    const frames = new Map<number, FrameRequestCallback>();
    let id = 0;
    let now = 0;
    const animator = createCastBarAnimator({
      now: () => now,
      request: (callback) => {
        frames.set(++id, callback);
        return id;
      },
      cancel: (frame) => frames.delete(frame),
    });
    let moving = 2;
    const paint = vi.fn(() => moving-- > 0);
    const stop = animator.subscribe(paint);
    expect(frames.size).toBe(1);
    const tick = () => {
      now += 16;
      const callbacks = [...frames.values()];
      frames.clear();
      for (const callback of callbacks) callback(now);
    };
    tick();
    expect(frames.size).toBe(1);
    tick();
    expect(frames.size).toBe(0);
    moving = 1;
    animator.wake();
    expect(frames.size).toBe(1);
    stop();
    expect(frames.size).toBe(0);
    animator.wake();
    expect(frames.size).toBe(0);
  });
});
