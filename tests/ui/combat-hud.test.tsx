import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import * as Tooltip from "@radix-ui/react-tooltip";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { CombatErrors } from "../../web/src/features/combat-errors/CombatErrors";
import { Frames } from "../../web/src/features/unit-frames/Frames";
import { initial } from "../../web/src/domain/preferences/schema";
import { preferences } from "../../web/src/state/config";
import { engine, error, focus, interaction } from "../../web/src/state/session";
import { snapshot } from "./fixtures";
import { activateSlot, dispatch, input } from "../../web/src/runtime/commands";
import {
  castError,
  clearCastFeedback,
  observeCastResult,
} from "../../web/src/runtime/cast-feedback";
import { ActionSlot } from "../../web/src/features/action-bars/ActionSlot";

beforeEach(() => {
  preferences.set(initial());
  clearCastFeedback();
  input.cancel();
  engine.set(snapshot());
  error.set("");
  focus.set("practice-opponent");
  interaction.set({ panel: null, quick: false, layout: false, hovered: null, capture: null });
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

it("gives every live and sample frame separate health and resource bars", () => {
  render(<Frames />);
  for (const role of ["player", "target", "focus", "opponent"]) {
    const health = screen.getByRole("meter", { name: `${role} health` });
    expect(within(health).getByText(role === "player" ? "100/100" : "75/100")).toBeTruthy();
    const resource = screen.getByRole("meter", { name: `${role} resource` });
    expect(within(resource).getByText("100/100")).toBeTruthy();
    expect(resource.getAttribute("aria-valuenow")).toBe("100");
    expect(resource.getAttribute("aria-valuetext")).toContain("not implemented");
  }
  expect(screen.queryByText(/Practice opponent|Equalized practice|selected|Defeated/)).toBeNull();
  expect(document.querySelector(".unit-frame .eyebrow")).toBeNull();
  expect(document.querySelector(".unit-frame small")).toBeNull();
  act(() => {
    focus.set(null);
    engine.set(null);
    interaction.set({ ...interaction.get(), layout: true });
  });
  expect(screen.getAllByRole("meter")).toHaveLength(16);
});

it("shows errors only after an attempted cast, refreshes repeated errors and expires them", () => {
  vi.useFakeTimers();
  render(<CombatErrors />);
  const area = screen.getByRole("status", { name: "Combat errors" });
  expect(area.textContent).toBe("");
  for (const attackStatus of ["Out of range", "Line of sight blocked", "Other rejection"]) {
    act(() => engine.set(snapshot({ attackStatus })));
    expect(area.textContent).toBe("");
    act(() => dispatch("slot:bar-1-s1"));
    expect(area.textContent).toBe(attackStatus);
    act(() => clearCastFeedback());
  }
  act(() => engine.set(snapshot({ selected: false })));
  expect(area.textContent).toBe("");
  act(() => activateSlot("bar-1-s1"));
  expect(area.textContent).toBe("Select a target");
  act(() => vi.advanceTimersByTime(2000));
  act(() => activateSlot("bar-1-s1"));
  act(() => vi.advanceTimersByTime(1000));
  expect(area.textContent).toBe("Select a target");
  act(() => vi.advanceTimersByTime(1500));
  expect(area.textContent).toBe("");
  act(() => engine.set(snapshot({ cooldown: 0.5 })));
  expect(area.textContent).toBe("");
  act(() => activateSlot("bar-1-s1"));
  expect(area.textContent).toContain("Recharging");
  act(() => engine.set(snapshot()));
  act(() => activateSlot("bar-1-s1"));
  expect(area.textContent).toBe("");
  act(() => error.set("Engine unavailable"));
  expect(area.textContent).toBe("");
  expect(screen.queryByRole("region", { name: "Practice round" })).toBeNull();
});

it("reports authoritative rejections only for submitted casts, not passive feedback", () => {
  observeCastResult(snapshot({ tick: 31, feedback: "Command rejected" }));
  expect(castError.get()).toBeNull();
  activateSlot("bar-1-s1");
  observeCastResult(snapshot({ tick: 30, feedback: "Command rejected" }));
  expect(castError.get()).toBeNull();
  observeCastResult(snapshot({ tick: 31, feedback: "Command rejected" }));
  expect(castError.get()?.message).toBe("Command rejected");
  activateSlot("bar-1-s1");
  observeCastResult(snapshot({ tick: 31, cooldown: 1 }));
  expect(castError.get()).toBeNull();
  activateSlot("bar-1-s1");
  observeCastResult(snapshot({ tick: 31 }));
  expect(castError.get()).toBeNull();
  activateSlot("bar-1-s1");
  observeCastResult(snapshot({ tick: 0 }));
  expect(castError.get()).toBeNull();
});

it("shares attempted-cast errors for clicks and keys, excluding empty slots and editing", () => {
  engine.set(snapshot({ attackStatus: "Line of sight blocked" }));
  render(
    <Tooltip.Provider>
      <ActionSlot slot={{ id: "bar-1-s1", ability: "pulse" }} number={1} />
      <CombatErrors />
    </Tooltip.Provider>,
  );
  const area = screen.getByRole("status", { name: "Combat errors" });
  act(() => activateSlot("bar-1-s2"));
  expect(area.textContent).toBe("");
  fireEvent.click(screen.getByRole("button", { name: "Slot 1 Pulse" }));
  expect(area.textContent).toBe("Line of sight blocked");
  act(() => {
    clearCastFeedback();
    interaction.set({ ...interaction.get(), layout: true });
    dispatch("slot:bar-1-s1");
  });
  expect(area.textContent).toBe("");
  act(() => {
    interaction.set({ ...interaction.get(), layout: false });
    dispatch("slot:bar-1-s1");
  });
  expect(area.textContent).toBe("Line of sight blocked");
  act(() => dispatch("reset"));
  expect(area.textContent).toBe("");
});
