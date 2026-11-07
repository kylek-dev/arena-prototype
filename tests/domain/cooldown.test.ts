import { describe, expect, it, vi } from "vitest";
import { CooldownTimeline } from "../../web/src/domain/abilities/cooldown-presentation";
import { needsImmediateDelivery } from "../../web/src/bridge/snapshot";
import { createCooldownAnimator } from "../../web/src/runtime/cooldown-animation";
import { createStore } from "../../web/src/state/store";
import { canActivate } from "../../web/src/domain/abilities/catalog";
import { snapshot } from "../ui/fixtures";

describe("cooldown presentation timeline", () => {
  it("reveals clockwise from the top as the authoritative fraction decreases", () => {
    const timeline = new CooldownTimeline();
    timeline.observe({ tick: 1, remaining: 1, duration: 1 }, 0);
    expect(timeline.visual(0).sweep).toBe(0);
    timeline.observe({ tick: 2, remaining: 0.75, duration: 1 }, 250);
    expect(timeline.visual(250).sweep).toBe(90);
    timeline.observe({ tick: 3, remaining: 0.5, duration: 1 }, 500);
    expect(timeline.visual(500).sweep).toBe(180);
    timeline.observe({ tick: 4, remaining: 0.25, duration: 1 }, 750);
    expect(timeline.visual(750).sweep).toBe(270);
    timeline.observe({ tick: 5, remaining: 0, duration: 1 }, 1000);
    expect(timeline.visual(1000)).toEqual({ active: false, sweep: 360, label: "" });
  });

  it("interpolates every frame and does not restart on repeated tick snapshots", () => {
    const timeline = new CooldownTimeline();
    const sample = { tick: 1, remaining: 1, duration: 1 };
    timeline.observe(sample, 0);
    expect(timeline.visual(16).sweep).toBeCloseTo(5.76);
    timeline.observe(sample, 20);
    expect(timeline.visual(32).sweep).toBeCloseTo(11.52);
    expect(timeline.visual(48).sweep).toBeGreaterThan(timeline.visual(32).sweep);
  });

  it("does not reverse a running sweep when quantized samples trail the estimate", () => {
    const timeline = new CooldownTimeline();
    timeline.observe({ tick: 1, remaining: 1, duration: 1 }, 0);
    const before = timeline.visual(80).sweep;
    timeline.observe({ tick: 2, remaining: 0.95, duration: 1 }, 80);
    expect(timeline.visual(80).sweep).toBeCloseTo(before);
    expect(timeline.visual(96).sweep).toBeGreaterThan(before);
  });

  it("never reports completion from a presentation clock and limits stale extrapolation", () => {
    const timeline = new CooldownTimeline();
    timeline.observe({ tick: 1, remaining: 0.05, duration: 1 }, 0);
    expect(timeline.visual(1000)).toMatchObject({ active: true, label: "0.1" });
    expect(timeline.visual(1000).sweep).toBeLessThan(360);
    timeline.observe({ tick: 2, remaining: 1, duration: 1 }, 1000);
    expect(timeline.visual(1150)).toEqual(timeline.visual(10000));
  });

  it("resets for a new cast, reset session, duration change and resumed snapshots", () => {
    const timeline = new CooldownTimeline();
    timeline.observe({ tick: 30, remaining: 0.2, duration: 1 }, 0);
    timeline.observe({ tick: 31, remaining: 1, duration: 1 }, 10);
    expect(timeline.visual(10).sweep).toBe(0);
    timeline.observe({ tick: 0, remaining: 0.8, duration: 1 }, 20);
    expect(timeline.visual(20).sweep).toBeCloseTo(72);
    timeline.observe({ tick: 1, remaining: 0.8, duration: 2 }, 30);
    expect(timeline.visual(30).sweep).toBe(216);
    timeline.observe({ tick: 2, remaining: 0.8, duration: 2 }, 1000);
    expect(timeline.visual(1000).sweep).toBe(216);
    timeline.observe(null, 1010);
    expect(timeline.visual(1010).active).toBe(false);
  });
});

it("bypasses HUD throttling for cooldown start/end/reset, not ordinary countdown samples", () => {
  const ready = snapshot();
  const cooling = snapshot({ cooldown: 1 });
  expect(needsImmediateDelivery(null, ready)).toBe(true);
  expect(needsImmediateDelivery(ready, cooling)).toBe(true);
  expect(needsImmediateDelivery(cooling, snapshot({ cooldown: 0.9 }))).toBe(false);
  expect(needsImmediateDelivery(cooling, ready)).toBe(true);
  expect(needsImmediateDelivery(cooling, snapshot({ tick: 0, cooldown: 0.5 }))).toBe(true);
});

it("shares one animation loop across slots and never mutates combat readiness", () => {
  const source = createStore(snapshot({ cooldown: 1 }));
  let now = 0;
  let id = 0;
  const frames = new Map<number, FrameRequestCallback>();
  const animator = createCooldownAnimator(source, {
    now: () => now,
    request(callback) {
      frames.set(++id, callback);
      return id;
    },
    cancel(frame) {
      frames.delete(frame);
    },
  });
  const first = vi.fn();
  const second = vi.fn();
  const stopFirst = animator.subscribe(first);
  const stopSecond = animator.subscribe(second);
  expect(frames.size).toBe(1);
  now = 16;
  const callbacks = [...frames.values()];
  frames.clear();
  for (const callback of callbacks) callback(now);
  expect(first.mock.lastCall).toEqual(second.mock.lastCall);
  expect(first.mock.lastCall?.[0].sweep).toBeCloseTo(5.76);
  expect(source.get().cooldown).toBe(1);
  expect(canActivate(source.get())).toBe(false);
  expect(frames.size).toBe(1);
  source.set(snapshot());
  expect(first.mock.lastCall?.[0].active).toBe(false);
  expect(canActivate(source.get())).toBe(true);
  expect(frames.size).toBe(0);
  source.set(snapshot({ cooldown: 1 }));
  stopFirst();
  expect(frames.size).toBe(1);
  stopSecond();
  expect(frames.size).toBe(0);
});
