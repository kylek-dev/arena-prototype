import * as Tooltip from "@radix-ui/react-tooltip";
import { Profiler } from "react";
import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { ActionSlot } from "../../web/src/features/action-bars/ActionSlot";
import { engine, interaction } from "../../web/src/state/session";
import { snapshot } from "./fixtures";

const frames = new Map<number, FrameRequestCallback>();
let now = 0;
let id = 0;

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
  engine.set(snapshot({ cooldown: 1 }));
  interaction.set({ panel: null, capture: null, quick: false, layout: false, hovered: null });
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("paints synchronized clockwise progress between snapshots without rerendering the slots", () => {
  const commits = vi.fn();
  const { container } = render(
    <Tooltip.Provider>
      <Profiler id="slots" onRender={commits}>
        <ActionSlot slot={{ id: "bar-1-s1", ability: "pulse" }} number={1} />
        <ActionSlot slot={{ id: "bar-1-s2", ability: "pulse" }} number={2} />
      </Profiler>
    </Tooltip.Provider>,
  );
  const layers = container.querySelectorAll<HTMLElement>(".cooldown");
  expect(layers).toHaveLength(2);
  expect(layers[0].style.getPropertyValue("--cooldown-sweep")).toBe("0deg");
  const renderCount = commits.mock.calls.length;
  act(() => {
    now = 16;
    const callbacks = [...frames.values()];
    frames.clear();
    for (const callback of callbacks) callback(now);
  });
  expect(parseFloat(layers[0].style.getPropertyValue("--cooldown-sweep"))).toBeCloseTo(5.76);
  expect(layers[0].style.getPropertyValue("--cooldown-sweep")).toBe(
    layers[1].style.getPropertyValue("--cooldown-sweep"),
  );
  expect(commits).toHaveBeenCalledTimes(renderCount);
  expect(engine.get()?.cooldown).toBe(1);
  act(() => {
    engine.set(snapshot());
  });
  expect(container.querySelector(".cooldown")).toBeNull();
  expect(frames.size).toBe(0);
});
