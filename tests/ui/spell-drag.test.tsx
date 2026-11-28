import * as Tooltip from "@radix-ui/react-tooltip";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it } from "vitest";
import { initial } from "../../web/src/domain/preferences/schema";
import { preferences, persistence, updateConfig } from "../../web/src/state/config";
import {
  draggingSpell,
  pressingSpell,
  engine,
  error,
  interaction,
} from "../../web/src/state/session";
import { ActionBars } from "../../web/src/features/action-bars/ActionBars";
import { Spellbook } from "../../web/src/features/spellbook/Spellbook";
import { SpellDragPreview } from "../../web/src/features/spellbook/SpellDragPreview";
import { installInput } from "../../web/src/runtime/input-router";
import { installPointer } from "../../web/src/runtime/pointer";
import { endDrag, installSpellDragging } from "../../web/src/runtime/spell-drag";
import { input } from "../../web/src/runtime/commands";
import { hitTarget, moveSpell, dropSpell } from "./drag-fixture";
import { snapshot } from "./fixtures";

let canvas: HTMLCanvasElement;
let router: ReturnType<typeof installInput>;
let pointer: ReturnType<typeof installPointer>;
let stop: () => void;

beforeEach(() => {
  document.body.innerHTML = '<canvas id="arena" tabindex="0"></canvas>';
  canvas = document.getElementById("arena") as HTMLCanvasElement;
  preferences.set(initial());
  engine.set(snapshot());
  error.set("");
  interaction.set({
    panel: "spellbook",
    capture: null,
    quick: false,
    layout: false,
    hovered: null,
  });
  endDrag();
  input.cancel();
  pointer = installPointer(canvas, () => router.cancel());
  router = installInput(canvas, pointer.end);
  stop = installSpellDragging(canvas);
  canvas.focus();
  render(
    <Tooltip.Provider>
      <ActionBars />
      <Spellbook />
      <SpellDragPreview />
    </Tooltip.Provider>,
  );
});

afterEach(() => {
  stop();
  router.dispose();
  pointer.dispose();
  cleanup();
  clearTimeout(persistence.timer);
});

function press(source: HTMLElement, shiftKey = false) {
  return fireEvent.pointerDown(source, {
    pointerId: 1,
    button: 0,
    clientX: 100,
    clientY: 100,
    shiftKey,
  });
}

it("uses the shared 64px dimension for bar tracks and spellbook/slot icons", () => {
  const bar = screen.getByRole("region", { name: "Action bar bar-1" });
  expect(bar.style.gridTemplateColumns).toContain("var(--action-slot-size, 64px)");
  expect(
    screen.getByRole("button", { name: "Pulse spell" }).classList.contains("action-slot"),
  ).toBe(true);
  expect(
    screen.getByRole("button", { name: "Slot 1 Pulse" }).classList.contains("action-slot"),
  ).toBe(true);
});

it("copies from the spellbook onto an empty slot without native dragstart or a cast", async () => {
  const source = screen.getByRole("button", { name: "Pulse spell" });
  const destination = screen.getByRole("button", { name: "Slot 5 empty" });
  hitTarget(destination);
  expect(press(source)).toBe(false);
  expect(pressingSpell.get()).toBe(true);
  act(() => {
    source.focus();
  });
  await act(async () => {
    await Promise.resolve();
  });
  expect(document.activeElement).toBe(source);
  moveSpell();
  expect(draggingSpell.get()).toBe(true);
  expect(destination.classList.contains("drop-target")).toBe(true);
  expect(document.querySelector(".spell-drag-preview")).toBeTruthy();
  dropSpell();
  fireEvent.click(source);
  expect(preferences.get().profiles[0].config.bars[0].slots[4].ability).toBe("pulse");
  expect(preferences.get().profiles[0].config.bars[0].slots[0].ability).toBe("pulse");
  expect(router.sample()[2]).toBe(0);
  expect(input.dragging).toBe(false);
  expect(document.querySelector(".spell-drag-preview")).toBeNull();
});

it("centres the drag preview on the pointer", () => {
  hitTarget(null);
  press(screen.getByRole("button", { name: "Pulse spell" }));
  moveSpell(240, 180);
  const preview = document.querySelector<HTMLElement>(".spell-drag-preview")!;
  expect([preview.style.left, preview.style.top]).toEqual(["240px", "180px"]);
  dropSpell();
});

it("hides empty slots, when set, until a spell is dragged", () => {
  const bar = screen.getByRole("region", { name: "Action bar bar-1" });
  expect(bar.classList.contains("hide-empty")).toBe(false);
  act(() => updateConfig((c) => ({ ...c, showEmptySlots: false })));
  expect(bar.classList.contains("hide-empty")).toBe(true);
  hitTarget(null);
  press(screen.getByRole("button", { name: "Pulse spell" }));
  moveSpell();
  expect(bar.classList.contains("hide-empty")).toBe(false);
  dropSpell();
  expect(bar.classList.contains("hide-empty")).toBe(true);
  // The layout and key binding editors show them too.
  act(() => interaction.set({ ...interaction.get(), quick: true }));
  expect(bar.classList.contains("hide-empty")).toBe(false);
});

it("moves a slot assignment and discards it on canvas without deleting the binding", () => {
  const bindings = preferences.get().profiles[0].config.bindings;
  hitTarget(screen.getByRole("button", { name: "Slot 5 empty" }));
  press(screen.getByRole("button", { name: "Slot 1 Pulse" }));
  moveSpell();
  dropSpell();
  expect([
    preferences.get().profiles[0].config.bars[0].slots[0].ability,
    preferences.get().profiles[0].config.bars[0].slots[4].ability,
  ]).toEqual([null, "pulse"]);
  hitTarget(canvas);
  press(screen.getByRole("button", { name: "Slot 5 Pulse" }));
  moveSpell();
  dropSpell();
  expect(preferences.get().profiles[0].config.bars[0].slots[4].ability).toBeNull();
  expect(preferences.get().profiles[0].config.bindings).toBe(bindings);
  expect(router.sample().slice(0, 6)).toEqual([0, 0, 0, 0, 0, 0]);
});

it("Shift copies a slot and an invalid drop does not remove the source", () => {
  hitTarget(screen.getByRole("button", { name: "Slot 5 empty" }));
  press(screen.getByRole("button", { name: "Slot 1 Pulse" }), true);
  moveSpell();
  dropSpell();
  expect([
    preferences.get().profiles[0].config.bars[0].slots[0].ability,
    preferences.get().profiles[0].config.bars[0].slots[4].ability,
  ]).toEqual(["pulse", "pulse"]);
  hitTarget(null);
  press(screen.getByRole("button", { name: "Slot 1 Pulse" }));
  moveSpell();
  dropSpell();
  expect(preferences.get().profiles[0].config.bars[0].slots[0].ability).toBe("pulse");
});

it("accepts a fast release even when pointermove was coalesced", () => {
  hitTarget(screen.getByRole("button", { name: "Slot 5 empty" }));
  press(screen.getByRole("button", { name: "Rift Bolt spell" }));
  dropSpell();
  expect(preferences.get().profiles[0].config.bars[0].slots[4].ability).toBe("rift_bolt");
  expect(router.sample()[2]).toBe(0);
});

it("can arrange a spell when it cannot currently be cast", () => {
  act(() => engine.set(snapshot({ selected: false, cooldown: 0.5 })));
  const source = screen.getByRole("button", { name: "Slot 1 Pulse" });
  expect(source.getAttribute("aria-disabled")).toBe("true");
  hitTarget(screen.getByRole("button", { name: "Slot 5 empty" }));
  press(source);
  moveSpell();
  dropSpell();
  expect(preferences.get().profiles[0].config.bars[0].slots[4].ability).toBe("pulse");
  expect(engine.get()?.cooldown).toBe(0.5);
  expect(router.sample()[2]).toBe(0);
});

it("does not mutate a different profile or a stale source on release", () => {
  hitTarget(screen.getByRole("button", { name: "Slot 5 empty" }));
  press(screen.getByRole("button", { name: "Slot 1 Pulse" }));
  moveSpell();
  const p = preferences.get();
  act(() =>
    preferences.set({
      ...p,
      active: "other",
      profiles: [
        ...p.profiles,
        { id: "other", name: "Other", config: structuredClone(p.profiles[0].config) },
      ],
    }),
  );
  dropSpell();
  expect(preferences.get().profiles[1].config.bars[0].slots[4].ability).toBeNull();
  hitTarget(screen.getByRole("button", { name: "Slot 5 empty" }));
  press(screen.getByRole("button", { name: "Slot 1 Pulse" }));
  moveSpell();
  act(() =>
    updateConfig((c) => ({
      ...c,
      bars: c.bars.map((b) => ({
        ...b,
        slots: b.slots.map((s) => (s.id === "bar-1-s1" ? { ...s, ability: null } : s)),
      })),
    })),
  );
  const before = preferences.get();
  dropSpell();
  expect(preferences.get()).toBe(before);
  expect(preferences.get().profiles[1].config.bars[0].slots[4].ability).toBeNull();
});

it("provides an explicit unlock for saved locked profiles without silently changing it", async () => {
  const user = userEvent.setup();
  act(() => {
    updateConfig((c) => ({ ...c, assignmentsLocked: true }));
  });
  press(screen.getByRole("button", { name: "Pulse spell" }));
  expect(pressingSpell.get()).toBe(false);
  expect(preferences.get().profiles[0].config.assignmentsLocked).toBe(true);
  await user.click(screen.getByRole("button", { name: "Unlock spell dragging" }));
  expect(preferences.get().profiles[0].config.assignmentsLocked).toBe(false);
  expect(screen.queryByRole("button", { name: "Unlock spell dragging" })).toBeNull();
  hitTarget(screen.getByRole("button", { name: "Slot 5 empty" }));
  press(screen.getByRole("button", { name: "Pulse spell" }));
  moveSpell();
  dropSpell();
  expect(preferences.get().profiles[0].config.bars[0].slots[4].ability).toBe("pulse");
});
