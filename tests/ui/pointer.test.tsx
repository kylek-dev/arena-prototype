import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { initial } from "../../web/src/domain/preferences/schema";
import { preferences, persistence, updateConfig } from "../../web/src/state/config";
import { draggingSpell, engine, error, interaction } from "../../web/src/state/session";
import { snapshot } from "./fixtures";
import { installInput } from "../../web/src/runtime/input-router";
import { installPointer } from "../../web/src/runtime/pointer";
import { endDrag, installSpellDragging } from "../../web/src/runtime/spell-drag";
import { dispatch, input } from "../../web/src/runtime/commands";
import { armSpell, moveSpell, dropSpell, hitTarget } from "./drag-fixture";
import { fireEvent } from "@testing-library/react";

let router: ReturnType<typeof installInput>;
let pointer: ReturnType<typeof installPointer>;
let canvas: HTMLCanvasElement;
let lockedElement: Element | null = null;
let stopDrop: () => void;

beforeEach(() => {
  document.body.innerHTML = '<canvas tabindex="0"></canvas><button id="spell">Pulse</button>';
  canvas = document.querySelector("canvas")!;
  preferences.set(initial());
  engine.set(snapshot());
  error.set("");
  interaction.set({ panel: null, capture: null, quick: false, layout: false, hovered: null });
  input.cancel();
  endDrag();
  lockedElement = null;
  Object.defineProperty(document, "pointerLockElement", {
    configurable: true,
    get: () => lockedElement,
  });
  document.exitPointerLock = () => {
    lockedElement = null;
  };
  pointer = installPointer(canvas, () => router.cancel());
  router = installInput(canvas, pointer.end);
  stopDrop = installSpellDragging(canvas);
  hitTarget(canvas);
  canvas.focus();
});

afterEach(() => {
  router.dispose();
  pointer.dispose();
  endDrag();
  stopDrop();
  clearTimeout(persistence.timer);
});

function lock() {
  lockedElement = canvas;
  document.dispatchEvent(new Event("pointerlockchange"));
}

function unlock() {
  lockedElement = null;
  document.dispatchEvent(new Event("pointerlockchange"));
}

function walk() {
  window.dispatchEvent(new KeyboardEvent("keydown", { code: "KeyW", bubbles: true }));
}

function move(buttons: number, movementX: number) {
  const event = new MouseEvent("mousemove", { buttons });
  Object.defineProperties(event, {
    movementX: { value: movementX },
    movementY: { value: 0 },
  });
  document.dispatchEvent(event);
}

it.each([0, 2])("button %i clicks never capture the cursor or activate look modes", (button) => {
  canvas.requestPointerLock = vi.fn().mockResolvedValue(undefined);
  const buttons = button === 0 ? 1 : 2;
  canvas.dispatchEvent(new MouseEvent("mousedown", { button, buttons, clientX: 25, clientY: 40 }));
  move(buttons, 2);
  expect(router.sample().slice(11, 13)).toEqual([0, 0]);
  expect(canvas.style.cursor).toBe("");
  document.dispatchEvent(new MouseEvent("mouseup", { button, buttons: 0 }));
  expect(canvas.requestPointerLock).not.toHaveBeenCalled();
  const sample = router.sample();
  expect(sample.slice(9, 11)).toEqual(button === 0 ? [25, 40] : [-1, -1]);
  expect(sample.slice(15, 17)).toEqual(button === 2 ? [25, 40] : [-1, -1]);
  expect(sample.slice(11, 13)).toEqual([0, 0]);
});

it.each([0, 2])("button %i captures only once after crossing the drag threshold", (button) => {
  canvas.requestPointerLock = vi.fn().mockResolvedValue(undefined);
  const buttons = button === 0 ? 1 : 2;
  canvas.dispatchEvent(new MouseEvent("mousedown", { button, buttons }));
  move(buttons, 2);
  expect(canvas.requestPointerLock).not.toHaveBeenCalled();
  move(buttons, 2);
  move(buttons, 10);
  expect(canvas.requestPointerLock).toHaveBeenCalledTimes(1);
  expect(canvas.style.cursor).toBe("none");
  expect(router.sample().slice(11, 13)).toEqual(button === 0 ? [0, 1] : [1, 0]);
  document.dispatchEvent(new MouseEvent("mouseup", { button, buttons: 0 }));
  expect(canvas.style.cursor).toBe("");
  const released = router.sample();
  expect(released.slice(9, 11)).toEqual([-1, -1]);
  expect(released.slice(15, 17)).toEqual([-1, -1]);
  move(0, 10);
  expect(canvas.requestPointerLock).toHaveBeenCalledTimes(1);
});

it("a stationary two-button chord still captures immediately to walk and steer", () => {
  canvas.requestPointerLock = vi.fn().mockResolvedValue(undefined);
  canvas.dispatchEvent(new MouseEvent("mousedown", { button: 0, buttons: 1 }));
  canvas.dispatchEvent(new MouseEvent("mousedown", { button: 2, buttons: 3 }));
  expect(canvas.requestPointerLock).toHaveBeenCalledTimes(1);
  expect(router.sample()[1]).toBe(-1);
});

it("ordinary mouse release and unlock preserve held keyboard movement", () => {
  walk();
  canvas.dispatchEvent(new MouseEvent("mousedown", { button: 2, buttons: 2 }));
  lock();
  document.dispatchEvent(new MouseEvent("mouseup", { button: 2, buttons: 0 }));
  unlock();
  expect(router.sample()[1]).toBe(-1);
});

it("unexpected lock loss cancels both keyboard and mouse input", () => {
  walk();
  canvas.dispatchEvent(new MouseEvent("mousedown", { button: 2, buttons: 2 }));
  lock();
  unlock();
  expect(router.sample().slice(0, 6)).toEqual([0, 0, 0, 0, 0, 0]);
  expect(input.dragging).toBe(false);
});

it.each(["keyboard reset", "command reset", "round end", "engine reset", "late lock"])(
  "%s releases capture and clears steering without losing reset intent",
  (transition) => {
    canvas.requestPointerLock = vi.fn().mockResolvedValue(undefined);
    walk();
    canvas.dispatchEvent(new MouseEvent("mousedown", { button: 2, buttons: 2 }));
    move(2, 10);
    if (transition !== "late lock") lock();
    expect(canvas.style.cursor).toBe("none");
    expect(input.looking).toBe(true);
    if (transition === "keyboard reset") {
      window.dispatchEvent(new KeyboardEvent("keydown", { code: "KeyR", bubbles: true }));
    } else if (transition === "round end") {
      engine.set(snapshot({ won: true }));
    } else if (transition === "engine reset") {
      engine.set(snapshot({ tick: 0 }));
    } else {
      dispatch("reset");
    }
    if (transition === "late lock") lock();
    expect(lockedElement).toBeNull();
    expect(canvas.style.cursor).toBe("");
    expect(input.dragging).toBe(false);
    expect(input.looking).toBe(false);
    const sample = router.sample();
    expect(sample.slice(0, 5)).toEqual([0, 0, 0, 0, 0]);
    expect(sample[5]).toBe(transition === "round end" || transition === "engine reset" ? 0 : 1);
    expect(sample.slice(11, 13)).toEqual([0, 0]);
    expect(router.sample()[5]).toBe(0);
    // Trailing motion/release cannot resurrect the old gesture or select/focus.
    move(2, 10);
    document.dispatchEvent(new MouseEvent("mouseup", { button: 2, buttons: 0 }));
    expect(router.sample().slice(15, 17)).toEqual([-1, -1]);
    expect(canvas.style.cursor).toBe("");
    canvas.dispatchEvent(new MouseEvent("mousedown", { button: 2, buttons: 2 }));
    move(2, 10);
    expect(input.looking).toBe(true);
    expect(canvas.style.cursor).toBe("none");
  },
);

it("drag transactions clear held movement, suppress casts and cancel without placement", () => {
  walk();
  armSpell(document.getElementById("spell")!, { ability: "pulse", source: "bar-1-s1" });
  moveSpell();
  expect(draggingSpell.get()).toBe(true);
  window.dispatchEvent(new KeyboardEvent("keydown", { code: "Digit1" }));
  expect(router.sample().slice(0, 6)).toEqual([0, 0, 0, 0, 0, 0]);
  endDrag();
  expect(preferences.get().profiles[0].config.bars[0].slots[0].ability).toBe("pulse");
});

it("reset also clears two-button walking when pointer lock is unavailable", () => {
  canvas.requestPointerLock = vi.fn(() => {
    throw new Error("Pointer lock denied");
  });
  canvas.dispatchEvent(new MouseEvent("mousedown", { button: 0, buttons: 1 }));
  canvas.dispatchEvent(new MouseEvent("mousedown", { button: 2, buttons: 3 }));
  expect(router.sample()[1]).toBe(-1);
  expect(canvas.style.cursor).toBe("none");
  dispatch("reset");
  expect(canvas.style.cursor).toBe("");
  expect(input.mouseButtons).toBe(0);
  const sample = router.sample();
  expect(sample.slice(0, 6)).toEqual([0, 0, 0, 0, 0, 1]);
  expect(sample.slice(11, 13)).toEqual([0, 0]);
});

it("exclusive UI modes prevent canvas gestures", () => {
  interaction.set({ ...interaction.get(), layout: true });
  canvas.dispatchEvent(new MouseEvent("mousedown", { button: 0, buttons: 1 }));
  expect(input.dragging).toBe(false);
  canvas.dispatchEvent(new MouseEvent("mousedown", { button: 2, buttons: 2 }));
  document.dispatchEvent(new MouseEvent("mouseup", { button: 2, buttons: 0 }));
  expect(router.sample().slice(15, 17)).toEqual([-1, -1]);
});

it("dropping a slotted spell on canvas removes its assignment, not its binding", () => {
  const bindings = preferences.get().profiles[0].config.bindings;
  armSpell(document.getElementById("spell")!, { ability: "pulse", source: "bar-1-s1" });
  moveSpell();
  dropSpell();
  expect(preferences.get().profiles[0].config.bars[0].slots[0].ability).toBeNull();
  expect(preferences.get().profiles[0].config.bindings).toBe(bindings);
  expect(draggingSpell.get()).toBe(false);
  expect(router.sample().slice(0, 6)).toEqual([0, 0, 0, 0, 0, 0]);
});

it("catalog-to-canvas and cancelled drops are no-ops", () => {
  const bars = preferences.get().profiles[0].config.bars;
  armSpell(document.getElementById("spell")!, { ability: "pulse" });
  moveSpell();
  dropSpell();
  expect(preferences.get().profiles[0].config.bars).toBe(bars);
  armSpell(document.getElementById("spell")!, { ability: "pulse", source: "bar-1-s1" });
  moveSpell();
  endDrag();
  dropSpell();
  expect(preferences.get().profiles[0].config.bars).toBe(bars);
});

it("does not discard an assignment if it becomes locked during a drag", () => {
  armSpell(document.getElementById("spell")!, { ability: "pulse", source: "bar-1-s1" });
  moveSpell();
  updateConfig((c) => ({ ...c, assignmentsLocked: true }));
  dropSpell();
  expect(preferences.get().profiles[0].config.bars[0].slots[0].ability).toBe("pulse");
});

it("Escape and pointer cancellation leave assignments unchanged and suppress the trailing click", () => {
  const source = document.getElementById("spell")!;
  source.addEventListener("click", () => input.requestAttack());
  armSpell(source, { ability: "pulse", source: "bar-1-s1" });
  moveSpell();
  window.dispatchEvent(new KeyboardEvent("keydown", { code: "Escape", cancelable: true }));
  fireEvent.click(source);
  expect(router.sample()[2]).toBe(0);
  expect(preferences.get().profiles[0].config.bars[0].slots[0].ability).toBe("pulse");
  armSpell(source, { ability: "pulse", source: "bar-1-s1" });
  moveSpell();
  fireEvent.pointerCancel(document, { pointerId: 1 });
  dropSpell();
  expect(preferences.get().profiles[0].config.bars[0].slots[0].ability).toBe("pulse");
  expect(draggingSpell.get()).toBe(false);
});

it("leaves a below-threshold press as a click rather than removing the spell", () => {
  const source = document.getElementById("spell")!;
  source.addEventListener("click", () => input.requestAttack());
  armSpell(source, { ability: "pulse", source: "bar-1-s1" });
  moveSpell(102, 101);
  expect(draggingSpell.get()).toBe(false);
  dropSpell(false, 102, 101);
  fireEvent.click(source);
  expect(router.sample()[2]).toBe(1);
  expect(preferences.get().profiles[0].config.bars[0].slots[0].ability).toBe("pulse");
});
