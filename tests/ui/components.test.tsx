import * as Tooltip from "@radix-ui/react-tooltip";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it } from "vitest";
import { Modal } from "../../web/src/ui/Dialog";
import { UnitFrame } from "../../web/src/features/unit-frames/UnitFrame";
import { ActionSlot } from "../../web/src/features/action-bars/ActionSlot";
import { Spellbook } from "../../web/src/features/spellbook/Spellbook";
import { BindingsEditor, BindingConflict } from "../../web/src/features/keybindings/BindingsEditor";
import { ProfileSettings } from "../../web/src/features/settings/ProfileSettings";
import { BarSettings } from "../../web/src/features/settings/BarSettings";
import { Loading } from "../../web/src/features/loading/Loading";
import { Frames } from "../../web/src/features/unit-frames/Frames";
import { LayoutSettings, LayoutToolbar } from "../../web/src/features/layout-editor/LayoutSettings";
import { initial, defaultConfig, validateConfig } from "../../web/src/domain/preferences/schema";
import { input, interactEntity } from "../../web/src/runtime/commands";
import { engine, error, focus, interaction, selectedLayout } from "../../web/src/state/session";
import { preferences, persistence, saveMessage } from "../../web/src/state/config";
import { acceptBinding, pendingBinding } from "../../web/src/state/binding-capture";
import { chord } from "../../web/src/domain/bindings/model";
import { snapshot } from "./fixtures";
import { moveSpell, dropSpell, hitTarget } from "./drag-fixture";
import { endDrag, installSpellDragging } from "../../web/src/runtime/spell-drag";

beforeEach(() => {
  preferences.set(initial());
  engine.set(snapshot());
  focus.set(null);
  error.set("");
  saveMessage.set("");
  interaction.set({ panel: null, quick: false, layout: false, hovered: null, capture: null });
  pendingBinding.set(null);
  selectedLayout.set(null);
  input.select = false;
});

afterEach(() => {
  cleanup();
  endDrag();
  clearTimeout(persistence.timer);
});

it("uses one unit frame for empty, dead, selected and focused units", () => {
  const { rerender } = render(<UnitFrame role="target" unit={null} />);
  expect(screen.getByRole("region", { name: "target status" })).toBeTruthy();
  const unit = { id: "e", name: "Enemy Bot", health: 0, maxHealth: 100 };
  rerender(<UnitFrame role="focus" unit={unit} />);
  expect(screen.getByText("Enemy Bot")).toBeTruthy();
  expect(screen.getByText("0/100")).toBeTruthy();
  expect(screen.queryByText("Defeated")).toBeNull();
  expect(screen.getByRole("meter", { name: "focus health" }).getAttribute("aria-valuenow")).toBe(
    "0",
  );
});

it("traps modal focus and returns it to the opener", async () => {
  const user = userEvent.setup();
  const opener = document.createElement("button");
  opener.textContent = "Opener";
  document.body.append(opener);
  opener.focus();
  let closed = false;
  const { unmount } = render(
    <Modal
      title="Example"
      close={() => {
        closed = true;
      }}
    >
      <button>Inside</button>
    </Modal>,
  );
  expect(screen.getByRole("dialog").contains(document.activeElement)).toBe(true);
  await user.tab({ shift: true });
  expect(screen.getByRole("dialog").contains(document.activeElement)).toBe(true);
  await user.keyboard("{Escape}");
  expect(closed).toBe(true);
  unmount();
  await waitFor(() => expect(document.activeElement).toBe(opener));
  opener.remove();
});

it("has an icon-only draggable spellbook with no assignment forms", () => {
  render(
    <Tooltip.Provider>
      <Spellbook />
      <ActionSlot slot={{ id: "bar-1-s2", ability: null }} number={2} />
    </Tooltip.Provider>,
  );
  const spell = screen.getByRole("button", { name: "Pulse spell" });
  expect(spell.getAttribute("data-spell-draggable")).toBe("true");
  expect(spell.classList.contains("action-slot")).toBe(true);
  expect(screen.queryByRole("combobox")).toBeNull();
  expect(screen.queryByRole("button", { name: "Place Pulse" })).toBeNull();
  const canvas = document.createElement("canvas");
  const stop = installSpellDragging(canvas);
  hitTarget(screen.getByRole("button", { name: "Slot 2 empty" }));
  fireEvent.pointerDown(spell, { pointerId: 1, button: 0, clientX: 100, clientY: 100 });
  moveSpell();
  dropSpell();
  stop();
  expect(preferences.get().profiles[0].config.bars[0].slots[1].ability).toBe("pulse");
  expect(preferences.get().profiles[0].config.bars[0].slots[0].ability).toBe("pulse");
});

it("shows keybindings without spell names or slot-number labels", () => {
  render(
    <Tooltip.Provider>
      <ActionSlot slot={{ id: "bar-1-s1", ability: "pulse" }} number={1} />
      <ActionSlot slot={{ id: "bar-1-s2", ability: null }} number={2} />
    </Tooltip.Provider>,
  );
  const assigned = screen.getByRole("button", { name: "Slot 1 Pulse" });
  const empty = screen.getByRole("button", { name: "Slot 2 empty" });
  expect(assigned.textContent).toBe("1ϟ");
  expect(assigned.querySelector("kbd")?.textContent).toBe("1");
  expect(assigned.querySelector("small")).toBeNull();
  expect(empty.textContent).toBe("·");
  expect(empty.querySelector("small")).toBeNull();
  expect(preferences.get().profiles[0].config.bindings["slot:bar-1-s1"][0]?.code).toBe("Digit1");
});

it("shows shared cooldown and selects quick-bind hover/focus", () => {
  engine.set(snapshot({ cooldown: 0.6 }));
  interaction.set({ ...interaction.get(), quick: true });
  render(
    <Tooltip.Provider>
      <ActionSlot slot={{ id: "bar-1-s1", ability: "pulse" }} number={1} />
    </Tooltip.Provider>,
  );
  const slot = screen.getByRole("button", { name: "Slot 1 Pulse" });
  expect(screen.getByText("0.6")).toBeTruthy();
  expect(slot.querySelector("kbd")?.textContent).toBe("1");
  expect(slot.querySelector("small")).toBeNull();
  fireEvent.mouseEnter(slot);
  expect(interaction.get().hovered).toBe("bar-1-s1");
  fireEvent.mouseLeave(slot);
  expect(interaction.get().hovered).toBeNull();
  fireEvent.focus(slot);
  expect(interaction.get().hovered).toBe("bar-1-s1");
});

it("tints target failures red and other failures grey, leaving missing targets and empty slots neutral", () => {
  render(
    <Tooltip.Provider>
      <ActionSlot slot={{ id: "bar-1-s1", ability: "pulse" }} number={1} />
      <ActionSlot slot={{ id: "bar-1-s2", ability: null }} number={2} />
    </Tooltip.Provider>,
  );
  const slot = screen.getByRole("button", { name: "Slot 1 Pulse" });
  const empty = screen.getByRole("button", { name: "Slot 2 empty" });
  expect(slot.hasAttribute("data-unusable")).toBe(false);
  act(() => engine.set(snapshot({ selected: false, distance: 7 })));
  expect(slot.hasAttribute("data-unusable")).toBe(false);
  for (const overrides of [
    { distance: 7, cooldown: 0.5, attackStatus: "Recharging" },
    { attackStatus: "Out of range" },
    { attackStatus: "Line of sight blocked" },
    { won: true },
  ]) {
    act(() => engine.set(snapshot(overrides)));
    expect(slot.getAttribute("data-unusable")).toBe("target");
    expect(empty.hasAttribute("data-unusable")).toBe(false);
  }
  // Synthetic resource status exercises presentation only; resource rules do not exist yet.
  for (const overrides of [
    { cooldown: 0.5, attackStatus: "Recharging" },
    { attackStatus: "Insufficient resource" },
  ]) {
    act(() => engine.set(snapshot(overrides)));
    expect(slot.getAttribute("data-unusable")).toBe("unavailable");
  }
  act(() => engine.set(snapshot()));
  expect(slot.hasAttribute("data-unusable")).toBe(false);
  expect(slot.querySelector("kbd")?.textContent).toBe("1");
});

it("searches bindings, opens capture, clears alternate entries and cancels conflicts", async () => {
  const user = userEvent.setup();
  render(
    <>
      <BindingsEditor />
      <BindingConflict />
    </>,
  );
  await user.type(screen.getByLabelText("Search actions"), "forward");
  expect(screen.queryByRole("button", { name: "Bind back primary" })).toBeNull();
  await user.click(screen.getByRole("button", { name: "Bind forward secondary" }));
  expect(interaction.get().capture).toEqual({ action: "forward", entry: 1 });
  await user.click(screen.getByRole("button", { name: "Cancel capture" }));
  act(() => acceptBinding("back", 1, chord({ code: "KeyW" })));
  expect(screen.getByRole("dialog")).toBeTruthy();
  await user.click(screen.getByRole("button", { name: "Cancel" }));
  expect(preferences.get().profiles[0].config.bindings.forward[0]?.code).toBe("KeyW");
});

it("creates and switches profiles and reports invalid imports", async () => {
  const user = userEvent.setup();
  render(<ProfileSettings />);
  await user.type(screen.getByLabelText("Profile name"), "Other");
  await user.click(screen.getByRole("button", { name: "Create" }));
  expect(preferences.get().profiles).toHaveLength(2);
  await user.selectOptions(screen.getByLabelText("Active profile"), "default");
  expect(preferences.get().active).toBe("default");
  await user.type(screen.getByLabelText("Profile JSON (import / export)"), "invalid");
  await user.click(screen.getByRole("button", { name: "Import as new profiles" }));
  expect(screen.getByRole("status").textContent).toContain("SyntaxError");
});

it("adds and removes bounded bars and cleans stale bindings", async () => {
  const user = userEvent.setup();
  render(<BarSettings />);
  await user.click(screen.getByRole("button", { name: "Add action bar" }));
  const bar = preferences.get().profiles[0].config.bars[1];
  expect(preferences.get().profiles[0].config.bindings[`slot:${bar.slots[0].id}`]).toEqual([
    null,
    null,
  ]);
  await user.click(screen.getAllByRole("button", { name: "Remove bar" })[1]);
  expect(preferences.get().profiles[0].config.bindings[`slot:${bar.slots[0].id}`]).toBeUndefined();
});

it("renders actionable startup errors and a failed-character fallback notice", () => {
  error.set("Missing WASM");
  const { rerender } = render(<Loading />);
  expect(screen.getByRole("alert").textContent).toContain("Missing WASM");
  expect(screen.getByRole("button", { name: "Reload arena" })).toBeTruthy();
  act(() => {
    error.set("");
    engine.set(snapshot({ character: { status: "failed" } }));
  });
  rerender(<Loading />);
  expect(screen.getByRole("status").textContent).toContain("procedural fallback");
});

it("focus frame follows live health independently of actual target selection", async () => {
  engine.set(snapshot({ selected: false }));
  render(<Frames />);
  const opponent = screen.getByRole("button", { name: "Target Enemy Bot" });
  expect(fireEvent.contextMenu(opponent)).toBe(false);
  expect(focus.get()).toBe("practice-opponent");
  expect(input.select).toBe(false);
  act(() =>
    engine.set(snapshot({ selected: false, target: { ...snapshot().target, health: 50 } })),
  );
  expect(screen.queryByRole("region", { name: "target status" })).toBeNull();
  expect(
    within(screen.getByRole("region", { name: "focus status" })).getByText("50/100"),
  ).toBeTruthy();
  expect(within(opponent).getByText("50/100")).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Set focus" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Clear focus" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Target focus" })).toBeNull();
  fireEvent.click(opponent);
  expect(input.select).toBe(true);
  act(() => focus.set(null));
  expect(screen.queryByRole("region", { name: "focus status" })).toBeNull();
});

it("guards opponent interactions while editing, blocked, unavailable or stale", () => {
  render(<Frames />);
  const opponent = screen.getByRole("button", { name: "Target Enemy Bot" });
  for (const mode of [{ layout: true }, { quick: true }, { panel: "settings" as const }]) {
    act(() => interaction.set({ ...interaction.get(), ...mode }));
    fireEvent.click(opponent);
    fireEvent.contextMenu(opponent);
    expect(input.select).toBe(false);
    expect(focus.get()).toBeNull();
    act(() => interaction.set({ ...interaction.get(), layout: false, quick: false, panel: null }));
  }
  interactEntity("missing", "focus");
  expect(focus.get()).toBeNull();
  act(() => engine.set(null));
  interactEntity("practice-opponent", "target");
  expect(input.select).toBe(false);
});

it("adds opponent placement to older profiles without moving saved frames", () => {
  const defaults = defaultConfig();
  expect(defaults.frames.player.y).toBe(defaults.frames.target.y);
  expect(defaults.frames.player.x).toBeLessThan(defaults.frames.target.x);
  expect(defaults.frames.focus.x).toBeGreaterThan(defaults.frames.target.x);
  expect(defaults.frames.focus.x).toBeLessThan(defaults.frames.opponents.x);
  const saved = {
    player: { ...defaults.frames.player, x: 0.2 },
    target: defaults.frames.target,
    focus: defaults.frames.focus,
  };
  const restored = validateConfig({ ...defaults, frames: saved });
  expect(restored.frames.player).toEqual(saved.player);
  expect(restored.frames.opponents).toEqual(defaults.frames.opponents);
});

it("edits visibility and scale, previews hidden frames and resets selected layout", async () => {
  const user = userEvent.setup();
  render(
    <>
      <LayoutSettings />
      <Frames />
    </>,
  );
  await user.click(screen.getByRole("button", { name: "Edit on canvas" }));
  await user.click(screen.getByRole("button", { name: "Edit player" }));
  expect(selectedLayout.get()).toBe("player");
  expect(screen.queryByRole("combobox")).toBeNull();
  expect(screen.queryByLabelText("X anchor offset")).toBeNull();
  await user.click(screen.getByLabelText("Visible"));
  expect(preferences.get().profiles[0].config.frames.player.visible).toBe(false);
  fireEvent.change(screen.getByLabelText(/Scale/), { target: { value: "1.5" } });
  expect(preferences.get().profiles[0].config.frames.player.scale).toBe(1.5);
  await user.click(screen.getByRole("button", { name: "Reset selected" }));
  expect(preferences.get().profiles[0].config.frames.player.scale).toBe(1);
  expect(preferences.get().profiles[0].config.frames.player.visible).toBe(true);
  await user.click(screen.getByRole("button", { name: "Done" }));
  expect(interaction.get().layout).toBe(false);
});

it("shows sample frames without selection/focus and drags the whole selected frame", () => {
  engine.set(snapshot({ selected: false }));
  interaction.set({ ...interaction.get(), layout: true });
  render(<Frames />);
  expect(screen.getAllByRole("meter")).toHaveLength(16);
  expect(screen.getByRole("button", { name: "Edit opponents" })).toBeTruthy();
  const frame = screen.getByRole("button", { name: "Edit target" });
  const before = preferences.get().profiles[0].config.frames.target;
  fireEvent.pointerDown(frame, { button: 0, clientX: 600, clientY: 120, pointerId: 1 });
  expect(selectedLayout.get()).toBe("target");
  fireEvent.pointerMove(frame, { clientX: 400, clientY: 220, pointerId: 1 });
  expect(preferences.get().profiles[0].config.frames.target).toBe(before);
  fireEvent.pointerUp(frame, { pointerId: 1 });
  expect(preferences.get().profiles[0].config.frames.target.x).toBeLessThan(before.x);
  expect(preferences.get().profiles[0].config.frames.target.y).toBeGreaterThan(before.y);
  expect(screen.queryByRole("button", { name: "Move target" })).toBeNull();
});

it("moves the layout editor window by its title without changing a frame layout", () => {
  interaction.set({ ...interaction.get(), layout: true });
  render(<LayoutToolbar />);
  const window = screen.getByRole("complementary", { name: "Layout editor" });
  const title = screen.getByRole("button", { name: "Move Layout editor" });
  const before = window.style.left;
  const frames = preferences.get().profiles[0].config.frames;
  fireEvent.pointerDown(title, { button: 0, clientX: 400, clientY: 50, pointerId: 1 });
  fireEvent.pointerMove(title, { clientX: 500, clientY: 80, pointerId: 1 });
  fireEvent.pointerUp(title, { pointerId: 1 });
  expect(window.style.left).not.toBe(before);
  expect(preferences.get().profiles[0].config.frames).toBe(frames);
});
