import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import * as Tooltip from "@radix-ui/react-tooltip";
import { act, cleanup, render, screen } from "@testing-library/react";
import { ActionSlot } from "../../web/src/features/action-bars/ActionSlot";
import { bind, chord } from "../../web/src/domain/bindings/model";
import { initial } from "../../web/src/domain/preferences/schema";
import { preferences, updateConfig, persistence } from "../../web/src/state/config";
import { engine, error, focus, heldSlots, interaction } from "../../web/src/state/session";
import { pendingBinding } from "../../web/src/state/binding-capture";
import { installInput } from "../../web/src/runtime/input-router";
import { activateSlot, dispatch, input } from "../../web/src/runtime/commands";
import { snapshot } from "./fixtures";

let router: ReturnType<typeof installInput>;
let canvas: HTMLCanvasElement;
function key(code: string, options: KeyboardEventInit = {}) {
  const event = new KeyboardEvent("keydown", { code, bubbles: true, cancelable: true, ...options });
  window.dispatchEvent(event);
  return event;
}
function release(code: string) {
  window.dispatchEvent(new KeyboardEvent("keyup", { code }));
}

beforeEach(() => {
  document.body.innerHTML =
    '<canvas tabindex="0"></canvas><button id="menu-access">Menu</button><button id="hud">HUD button</button><input />';
  canvas = document.querySelector("canvas")!;
  preferences.set(initial());
  engine.set(snapshot());
  error.set("");
  interaction.set({ panel: null, quick: false, layout: false, hovered: null, capture: null });
  pendingBinding.set(null);
  focus.set(null);
  input.cancel();
  router = installInput(canvas, () => {});
  canvas.focus();
});

afterEach(() => {
  cleanup();
  router.dispose();
  clearTimeout(persistence.timer);
});

describe("production input routing", () => {
  it("highlights only the bound slot while held, even when casting is rejected or empty", () => {
    render(
      <Tooltip.Provider>
        <ActionSlot slot={{ id: "bar-1-s1", ability: "pulse" }} number={1} />
        <ActionSlot slot={{ id: "bar-1-s2", ability: null }} number={2} />
      </Tooltip.Provider>,
    );
    const slot = screen.getByRole("button", { name: "Slot 1 Pulse" });
    const empty = screen.getByRole("button", { name: "Slot 2 empty" });
    for (const state of [snapshot(), snapshot({ cooldown: 0.5 }), snapshot({ selected: false })]) {
      act(() => {
        engine.set(state);
        key("Digit1");
      });
      expect(slot.classList.contains("key-held")).toBe(true);
      expect(empty.classList.contains("key-held")).toBe(false);
      act(() => {
        key("Digit1", { repeat: true });
        release("KeyZ");
      });
      expect(slot.classList.contains("key-held")).toBe(true);
      act(() => release("Digit1"));
      expect(slot.classList.contains("key-held")).toBe(false);
    }
    act(() => {
      updateConfig((c) => ({
        ...c,
        bindings: bind(c.bindings, "slot:bar-1-s2", 0, chord({ code: "Digit2" })),
      }));
      key("Digit2");
    });
    expect(empty.classList.contains("key-held")).toBe(true);
    act(() => release("Digit2"));
    expect(empty.classList.contains("key-held")).toBe(false);
  });

  it("preserves alternate slot holds and clears highlights on cancellation and modifier release", () => {
    updateConfig((c) => ({
      ...c,
      bindings: bind(c.bindings, "slot:bar-1-s1", 1, chord({ code: "KeyE", shiftKey: true })),
    }));
    key("Digit1");
    key("KeyE", { shiftKey: true });
    release("Digit1");
    expect(heldSlots.get().has("bar-1-s1")).toBe(true);
    release("ShiftLeft");
    expect(heldSlots.get().size).toBe(0);
    key("Digit1");
    window.dispatchEvent(new Event("blur"));
    expect(heldSlots.get().size).toBe(0);
    key("Digit1");
    updateConfig((c) => ({ ...c }));
    expect(heldSlots.get().size).toBe(0);
    key("Digit1");
    document.querySelector("input")!.focus();
    expect(heldSlots.get().size).toBe(0);
    key("Digit1");
    expect(heldSlots.get().size).toBe(0);
    canvas.focus();
    interaction.set({ ...interaction.get(), quick: true });
    key("Digit1");
    expect(heldSlots.get().size).toBe(0);
  });

  it("maps movement, alternate bindings and unrelated releases to the existing ABI", () => {
    updateConfig((c) => ({
      ...c,
      bindings: bind(c.bindings, "forward", 1, chord({ code: "ArrowUp" })),
    }));
    key("KeyW");
    key("ArrowUp");
    release("KeyW");
    release("KeyD");
    expect(router.sample()[1]).toBe(-1);
    release("ArrowUp");
    expect(router.sample()[1]).toBe(0);
  });

  it("drains one-shot intent, ignores repeats and consumes repeated Tab", () => {
    key("Digit1");
    expect(router.sample()[2]).toBe(1);
    key("Digit1", { repeat: true });
    expect(router.sample()[2]).toBe(0);
    expect(key("Tab", { repeat: true }).defaultPrevented).toBe(true);
  });

  it("practice reset clears held movement and loading cannot submit gameplay", () => {
    key("KeyW");
    key("KeyR");
    expect(router.sample().slice(0, 6)).toEqual([0, 0, 0, 0, 0, 1]);
    release("KeyW");
    release("KeyR");
    engine.set(snapshot({ character: { status: "loading" } }));
    key("KeyW");
    key("Digit1");
    key("Tab");
    expect(router.sample().slice(0, 6)).toEqual([0, 0, 0, 0, 0, 0]);
  });

  it("capture consumes keys before movement, casting, targeting and reset", () => {
    key("KeyW");
    interaction.set({ ...interaction.get(), capture: { action: "clearFocus", entry: 0 } });
    expect(router.sample()[1]).toBe(0);
    key("Digit1");
    expect(pendingBinding.get()?.action).toBe("clearFocus");
    expect(router.sample().slice(0, 6)).toEqual([0, 0, 0, 0, 0, 0]);
    key("Escape");
    expect(pendingBinding.get()).toBeNull();
    key("Escape");
    expect(interaction.get().capture).toBeNull();
    expect(router.sample()[4]).toBe(0);
  });

  it("quick binding is only eligible over a slot and never casts", () => {
    interaction.set({ ...interaction.get(), quick: true });
    key("KeyE");
    expect(preferences.get().profiles[0].config.bindings["slot:bar-1-s1"][0]?.code).toBe("Digit1");
    interaction.set({ ...interaction.get(), hovered: "bar-1-s1" });
    key("KeyE", { shiftKey: true });
    expect(preferences.get().profiles[0].config.bindings["slot:bar-1-s1"][0]).toEqual(
      chord({ code: "KeyE", shiftKey: true }),
    );
    expect(router.sample()[2]).toBe(0);
    dispatch("reset");
    expect(router.sample()[5]).toBe(0);
    key("Escape");
    expect(interaction.get().quick).toBe(false);
  });

  it("typing, modal and layout modes isolate gameplay", () => {
    document.querySelector("input")!.focus();
    key("KeyR");
    key("Digit1");
    expect(router.sample().slice(0, 6)).toEqual([0, 0, 0, 0, 0, 0]);
    canvas.focus();
    for (const mode of [
      { panel: "settings" as const, layout: false },
      { panel: null, layout: true },
    ]) {
      interaction.set({ ...interaction.get(), ...mode });
      key("KeyW");
      key("Tab");
      expect(router.sample().slice(0, 6)).toEqual([0, 0, 0, 0, 0, 0]);
    }
  });

  it("clears held input on blur, binding changes and modifier release", () => {
    key("KeyW");
    window.dispatchEvent(new Event("blur"));
    expect(router.sample()[1]).toBe(0);
    key("KeyW");
    updateConfig((c) => ({ ...c }));
    expect(router.sample()[1]).toBe(0);
    updateConfig((c) => ({
      ...c,
      bindings: bind(c.bindings, "forward", 1, chord({ code: "KeyE", shiftKey: true })),
    }));
    key("KeyE", { shiftKey: true });
    expect(router.sample()[1]).toBe(-1);
    release("ShiftLeft");
    expect(router.sample()[1]).toBe(0);
  });

  it("Shift+Tab leaves the canvas and browser shortcuts remain untouched", () => {
    expect(key("Tab", { shiftKey: true }).defaultPrevented).toBe(true);
    expect(document.activeElement?.id).toBe("menu-access");
    canvas.focus();
    expect(key("KeyR", { ctrlKey: true }).defaultPrevented).toBe(false);
    expect(router.sample()[5]).toBe(0);
  });

  it("routes movement after an ordinary HUD button takes focus, without losing held movement", async () => {
    key("KeyW");
    document.getElementById("hud")!.focus();
    expect(router.sample()[1]).toBe(-1);
    await Promise.resolve();
    expect(document.activeElement).toBe(canvas);
    release("KeyW");
    document.getElementById("hud")!.focus();
    key("KeyD");
    expect(document.activeElement).toBe(canvas);
    expect(router.sample()[0]).toBe(1);
  });

  it("does not steal typing focus and restores canvas focus once the field blurs", async () => {
    // jsdom treats blurring the last element as document focus loss. Model a
    // still-focused browser window separately from its active DOM element.
    vi.spyOn(document, "hasFocus").mockReturnValue(true);
    key("KeyW");
    const field = document.querySelector("input")!;
    field.focus();
    expect(router.sample()[1]).toBe(0);
    key("Digit1");
    key("KeyR");
    await Promise.resolve();
    expect(document.activeElement).toBe(field);
    expect(router.sample().slice(0, 6)).toEqual([0, 0, 0, 0, 0, 0]);
    field.blur();
    await Promise.resolve();
    expect(document.activeElement).toBe(canvas);
  });

  it("opens the bindable Escape menu rather than clearing the target", () => {
    key("Escape");
    expect(interaction.get().panel).toBe("menu");
    expect(router.sample()[4]).toBe(0);
    release("Escape");
    key("Escape");
    expect(interaction.get().panel).toBeNull();
    release("Escape");
    key("Escape", { shiftKey: true });
    expect(router.sample()[4]).toBe(1);
  });

  it("can remap the menu and assign Escape back from the binding editor", () => {
    updateConfig((c) => ({ ...c, bindings: bind(c.bindings, "menu", 0, chord({ code: "KeyM" })) }));
    key("KeyM");
    expect(interaction.get().panel).toBe("menu");
    release("KeyM");
    key("KeyM");
    expect(interaction.get().panel).toBeNull();
    interaction.set({ ...interaction.get(), capture: { action: "menu", entry: 0 } });
    key("Escape");
    expect(preferences.get().profiles[0].config.bindings.menu[0]?.code).toBe("Escape");
    expect(interaction.get().capture).toBeNull();
  });

  it("allows gameplay with the icon-only spellbook open", () => {
    interaction.set({ ...interaction.get(), panel: "spellbook" });
    key("KeyW");
    key("Digit1");
    expect(router.sample().slice(0, 3)).toEqual([0, -1, 1]);
  });

  it("focus remains independent of target and uses current entity state", () => {
    dispatch("setFocus");
    expect(focus.get()).toBe("practice-opponent");
    dispatch("clearTarget");
    expect(router.sample()[4]).toBe(1);
    engine.set(snapshot({ selected: false }));
    dispatch("targetFocus");
    expect(router.sample()[3]).toBe(1);
    dispatch("reset");
    expect(focus.get()).toBeNull();
    expect(router.sample()[5]).toBe(1);
  });

  it("slot click and keyboard use the same assignment and authoritative availability", () => {
    activateSlot("bar-1-s1");
    expect(router.sample()[2]).toBe(1);
    engine.set(snapshot({ cooldown: 0.5 }));
    activateSlot("bar-1-s1");
    key("Digit1");
    expect(router.sample()[2]).toBe(0);
    release("Digit1");
    engine.set(snapshot({ selected: false }));
    key("Digit1");
    expect(router.sample()[2]).toBe(0);
    engine.set(snapshot());
    activateSlot("bar-1-s2");
    expect(router.sample()[2]).toBe(0);
  });
});
