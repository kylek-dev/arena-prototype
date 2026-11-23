import { Profiler } from "react";
import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { UnitFrame } from "../../web/src/features/unit-frames/UnitFrame";
import { Nameplates } from "../../web/src/features/nameplates/Nameplates";
import { engine, interaction, nameplateScene } from "../../web/src/state/session";
import type { Entity } from "../../web/src/domain/units/selection";
import { snapshot } from "./fixtures";

const frames = new Map<number, FrameRequestCallback>();
let now = 0;
let id = 0;

function step(ms: number) {
  act(() => {
    now += ms;
    const callbacks = [...frames.values()];
    frames.clear();
    for (const callback of callbacks) callback(now);
  });
}

const caster = (remainingTicks: number | null): Entity => ({
  id: "practice-opponent",
  name: "Enemy Bot",
  health: 75,
  maxHealth: 100,
  cast: remainingTicks === null ? null : { spell: "rift_bolt", remainingTicks, totalTicks: 45 },
});

beforeEach(() => {
  now = 0;
  id = 0;
  frames.clear();
  vi.spyOn(performance, "now").mockImplementation(() => now);
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
    frames.set(++id, callback);
    return id;
  });
  vi.stubGlobal("cancelAnimationFrame", (frame: number) => frames.delete(frame));
  engine.set(snapshot({ tickRate: 30 }));
  interaction.set({ panel: null, capture: null, quick: false, layout: false, hovered: null });
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("animates the unit-frame cast bar every frame without re-rendering React", () => {
  const commits = vi.fn();
  const { rerender } = render(
    <Profiler id="frame" onRender={commits}>
      <UnitFrame role="target" unit={caster(45)} />
    </Profiler>,
  );
  const bar = screen.getByRole("progressbar", { name: "target casting Rift Bolt" });
  const fill = bar.querySelector<HTMLElement>(".cast-fill")!;
  expect(fill.style.width).toBe("0%");
  expect(bar.querySelector(".cast-label")?.textContent).toBe("Rift Bolt1.5s");
  const renders = commits.mock.calls.length;
  step(16);
  const first = parseFloat(fill.style.width);
  step(17);
  const second = parseFloat(fill.style.width);
  expect(first).toBeGreaterThan(0);
  expect(second).toBeGreaterThan(first);
  expect(second).toBeCloseTo((0.033 / 1.5) * 100, 1);
  expect(commits).toHaveBeenCalledTimes(renders);

  // A throttled snapshot slightly behind the running bar does not pull it back.
  rerender(
    <Profiler id="frame" onRender={commits}>
      <UnitFrame role="target" unit={caster(44)} />
    </Profiler>,
  );
  expect(parseFloat(fill.style.width)).toBeGreaterThanOrEqual(second);

  // The cast ending removes the bar at once and stops the frame loop.
  rerender(
    <Profiler id="frame" onRender={commits}>
      <UnitFrame role="target" unit={caster(null)} />
    </Profiler>,
  );
  expect(screen.queryByRole("progressbar")).toBeNull();
  step(16);
  expect(frames.size).toBe(0);
});

it("snaps when the frame switches to another unit casting the same spell", () => {
  const { rerender } = render(<UnitFrame role="target" unit={caster(30)} />);
  const fill = () => document.querySelector<HTMLElement>(".cast-fill")!;
  step(16);
  rerender(<UnitFrame role="target" unit={{ ...caster(10), id: "other" }} />);
  expect(parseFloat(fill().style.width)).toBeCloseTo((35 / 45) * 100, 1);
});

it("snaps a new cast to its own start", () => {
  const { rerender } = render(<UnitFrame role="focus" unit={caster(5)} />);
  const fill = () => document.querySelector<HTMLElement>(".cast-fill")!;
  expect(parseFloat(fill().style.width)).toBeCloseTo((40 / 45) * 100, 1);
  rerender(<UnitFrame role="focus" unit={caster(45)} />);
  expect(fill().style.width).toBe("0%");
});

it("animates nameplate cast bars on the same loop", () => {
  nameplateScene.set(
    snapshot({
      tickRate: 30,
      target: {
        ...snapshot().target,
        cast: { spell: "rift_bolt", remainingTicks: 30, totalTicks: 45 },
      },
      nameplates: [{ tag: "Opponent1", x: 0.4, y: 0.3 }],
    }),
  );
  render(<Nameplates />);
  const fill = document.querySelector<HTMLElement>(".nameplate-cast-fill")!;
  const start = parseFloat(fill.style.width);
  expect(start).toBeCloseTo((15 / 45) * 100, 1);
  step(100);
  expect(parseFloat(fill.style.width)).toBeGreaterThan(start);
});
