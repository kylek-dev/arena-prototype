import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import * as Tooltip from "@radix-ui/react-tooltip";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { CombatErrors } from "../../web/src/features/combat-errors/CombatErrors";
import { Frames } from "../../web/src/features/unit-frames/Frames";
import { defaultConfig, initial, validateConfig } from "../../web/src/domain/preferences/schema";
import { layout } from "../../web/src/domain/layout/model";
import { preferences, updateConfig } from "../../web/src/state/config";
import { PlayerCastBar } from "../../web/src/features/cast-bar/PlayerCastBar";
import { engine, error, focus, interaction, selectedLayout } from "../../web/src/state/session";
import { LayoutSettings } from "../../web/src/features/layout-editor/LayoutSettings";
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

it("renders live unit resources, active cast bars, and lockout badges when present", () => {
  engine.set(
    snapshot({
      maxResource: 100,
      player: {
        ...snapshot().player,
        resource: 65,
        maxResource: 100,
        cast: { spell: "rift_bolt", remainingTicks: 15, totalTicks: 45 },
      },
      target: {
        ...snapshot().target,
        resource: 40,
        maxResource: 100,
        lockoutRemaining: 30,
        cast: null,
      },
    }),
  );
  render(
    <>
      <Frames />
      <PlayerCastBar />
    </>,
  );
  const playerResource = screen.getByRole("meter", { name: "player resource" });
  expect(within(playerResource).getByText("65/100")).toBeTruthy();
  expect(playerResource.getAttribute("aria-valuenow")).toBe("65");
  expect(playerResource.getAttribute("aria-valuetext")).toBe("65/100");
  const castBar = screen.getByRole("progressbar", { name: "player casting Rift Bolt" });
  expect(castBar.getAttribute("aria-valuenow")).toBe("67");
  expect(within(castBar).getByText("Rift Bolt")).toBeTruthy();
  expect(within(castBar).getByText("0.5s")).toBeTruthy();
  // By default it is its own movable frame, not part of the player frame.
  expect(castBar.closest(".player-cast-bar")).toBeTruthy();
  expect(castBar.closest(".unit-frame")).toBeNull();
  expect(screen.getAllByRole("progressbar")).toHaveLength(1);
  const targetResource = screen.getByRole("meter", { name: "target resource" });
  expect(within(targetResource).getByText("40/100")).toBeTruthy();
  expect(screen.getAllByText("Locked out").length).toBeGreaterThanOrEqual(1);
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
  act(() => activateSlot("bar-1-s5"));
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

it("activates all four default action-bar spells and allows Mend without an enemy target", () => {
  engine.set(snapshot());
  activateSlot("bar-1-s1");
  expect(input.readInput()[19]).toBe(1);
  activateSlot("bar-1-s2");
  expect(input.readInput()[19]).toBe(2);
  activateSlot("bar-1-s3");
  expect(input.readInput()[19]).toBe(3);
  engine.set(snapshot({ selected: false }));
  activateSlot("bar-1-s4");
  expect(input.readInput()[19]).toBe(4);
});

it("moves the player's cast bar into the player frame when attached, leaving other bars alone", () => {
  const casting = { spell: "mend", remainingTicks: 30, totalTicks: 45 } as const;
  engine.set(
    snapshot({
      player: { ...snapshot().player, cast: casting },
      target: { ...snapshot().target, cast: { ...casting, spell: "rift_bolt" } },
    }),
  );
  render(
    <>
      <Frames />
      <PlayerCastBar />
    </>,
  );
  const own = screen.getByRole("progressbar", { name: "player casting Mend" });
  expect(own.closest(".player-cast-bar")).toBeTruthy();
  const target = screen.getByRole("progressbar", { name: "target casting Rift Bolt" });
  expect(target.closest(".unit-frame.target")).toBeTruthy();

  act(() => updateConfig((c) => ({ ...c, castBarAttached: true })));
  const attached = screen.getByRole("progressbar", { name: "player casting Mend" });
  expect(attached.closest(".unit-frame.player")).toBeTruthy();
  expect(document.querySelector(".player-cast-bar")).toBeNull();
  expect(screen.getByRole("progressbar", { name: "target casting Rift Bolt" })).toBeTruthy();

  // Idle: nothing is shown in either place.
  act(() => {
    updateConfig((c) => ({ ...c, castBarAttached: false }));
    engine.set(snapshot());
  });
  expect(screen.queryAllByRole("progressbar")).toHaveLength(0);
});

it("shows a movable, scalable sample cast bar in the layout editor", () => {
  engine.set(snapshot());
  interaction.set({ panel: null, capture: null, quick: false, layout: true, hovered: null });
  render(<PlayerCastBar />);
  const placed = screen.getByRole("button", { name: "Edit castBar" });
  expect(placed.querySelector(".player-cast-bar .cast-track")).toBeTruthy();
  expect(defaultConfig().frames.castBar).toEqual(layout(0.5, 0.78));
  expect(defaultConfig().castBarAttached).toBe(false);
  act(() => interaction.set({ ...interaction.get(), layout: false }));
  expect(screen.queryByRole("button", { name: "Edit castBar" })).toBeNull();
});

it("keeps older saved profiles valid with the new frame and setting defaults", () => {
  const legacy = structuredClone(defaultConfig()) as Partial<ReturnType<typeof defaultConfig>>;
  delete (legacy.frames as Record<string, unknown>).castBar;
  delete legacy.castBarAttached;
  const restored = validateConfig(legacy);
  expect(restored.frames.castBar).toEqual(layout(0.5, 0.78));
  expect(restored.castBarAttached).toBe(false);
  expect(validateConfig({ ...defaultConfig(), castBarAttached: "yes" }).castBarAttached).toBe(
    false,
  );
});

it("keeps the live player cast bar pointer-transparent but draggable while editing", () => {
  // The pointer-events rule itself is checked in tests/unit/hud-css.test.mjs.
  engine.set(
    snapshot({
      player: { ...snapshot().player, cast: { spell: "mend", remainingTicks: 30, totalTicks: 45 } },
    }),
  );
  render(<PlayerCastBar />);
  expect(document.querySelector(".player-cast-bar-live")).toBeTruthy();
  act(() => interaction.set({ ...interaction.get(), layout: true }));
  expect(document.querySelector(".player-cast-bar")).toBeTruthy();
  expect(document.querySelector(".player-cast-bar-live")).toBeNull();
});

it("previews the attached bar in the player frame and drops a stale castBar selection", () => {
  engine.set(snapshot());
  interaction.set({ panel: null, capture: null, quick: false, layout: true, hovered: null });
  selectedLayout.set("castBar");
  render(
    <>
      <Frames />
      <PlayerCastBar />
      <LayoutSettings />
    </>,
  );
  expect(screen.getByText("Editing castBar")).toBeTruthy();
  fireEvent.click(screen.getByLabelText("Attach player cast bar to the player frame"));
  expect(selectedLayout.get()).toBeNull();
  expect(document.querySelector(".player-cast-bar")).toBeNull();
  const preview = screen.getByRole("progressbar", { name: "player casting Rift Bolt" });
  expect(preview.closest(".unit-frame.player")).toBeTruthy();
});
