import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it } from "vitest";
import { resolveUnit } from "../../web/src/domain/units/tags";
import { Frames } from "../../web/src/features/unit-frames/Frames";
import {
  activateSlot,
  dispatch,
  input,
  interactUnit,
  unitRoster,
} from "../../web/src/runtime/commands";
import { engine, error, focus, interaction } from "../../web/src/state/session";
import { snapshot } from "./fixtures";

beforeEach(() => {
  input.cancel();
  engine.set(snapshot());
  error.set("");
  focus.set(null);
  interaction.set({ panel: null, quick: false, layout: false, hovered: null, capture: null });
});
afterEach(cleanup);

it("resolves unit tags and leaves unpopulated future slots empty", () => {
  const state = snapshot();
  const roster = unitRoster(state, state.player.id);
  expect(resolveUnit("Player", roster)).toBe(state.player);
  expect(resolveUnit("Focus", roster)).toBe(state.player);
  expect(resolveUnit("Target", roster)).toBe(state.target);
  expect(resolveUnit("Opponent1", roster)).toBe(state.target);
  for (const tag of ["Opponent2", "Opponent3", "Team1", "Team2", "Team3"] as const)
    expect(resolveUnit(tag, roster)).toBeNull();
  expect(resolveUnit("Team2", { ...roster, team: [state.player, state.target] })).toBe(
    state.target,
  );
});

it("targets and focuses through every live frame, including the player", () => {
  render(<Frames />);
  for (const role of ["player", "target", "opponent"] as const) {
    input.cancel();
    const region = screen.getByRole("region", { name: `${role} status` });
    fireEvent.click(within(region).getByRole("meter", { name: `${role} health` }));
    const sample = input.readInput();
    expect(sample[role === "player" ? 13 : 3]).toBe(1);
    expect(fireEvent.contextMenu(region)).toBe(false);
    expect(focus.get()).toBe(role === "player" ? snapshot().player.id : snapshot().target.id);
    expect(input.select).toBe(false);
  }
  input.cancel();
  fireEvent.click(screen.getByRole("region", { name: "focus status" }));
  expect(input.select).toBe(true);
});

it("resolves self target/focus frames and guards absent tags and editing", () => {
  engine.set(snapshot({ selectedPlayer: true }));
  focus.set(snapshot().player.id);
  render(<Frames />);
  for (const role of ["target", "focus"]) {
    const region = screen.getByRole("region", { name: `${role} status` });
    expect(within(region).getByText("Player")).toBeTruthy();
    fireEvent.click(region);
    expect(input.readInput()[13]).toBe(1);
  }
  interactUnit("Team1", "target");
  expect(input.readInput()[13]).toBe(0);
  interaction.set({ ...interaction.get(), layout: true });
  interactUnit("Player", "target");
  expect(input.readInput()[13]).toBe(0);
});

it("self focus bindings preserve identity and self-targeted Pulse never queues an attack", () => {
  engine.set(snapshot({ selectedPlayer: true }));
  dispatch("setFocus");
  expect(focus.get()).toBe(snapshot().player.id);
  dispatch("targetFocus");
  expect(input.readInput()[13]).toBe(1);
  activateSlot("bar-1-s1");
  expect(input.readInput()[2]).toBe(0);
  dispatch("clearFocus");
  expect(focus.get()).toBeNull();
});
