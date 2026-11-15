import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it } from "vitest";
import { Frames } from "../../web/src/features/unit-frames/Frames";
import { GameMenu } from "../../web/src/features/menu/GameMenu";
import { dispatch, input } from "../../web/src/runtime/commands";
import { installInput } from "../../web/src/runtime/input-router";
import { engine, error, focus, interaction } from "../../web/src/state/session";
import { teamSnapshot } from "./fixtures";

beforeEach(() => {
  input.cancel();
  error.set("");
  focus.set(null);
  interaction.set({ panel: null, quick: false, layout: false, hovered: null, capture: null });
});
afterEach(cleanup);

it("marks only the targeted roster frame, not a focused unit, and clears selection", () => {
  engine.set(teamSnapshot(3, 2));
  focus.set("practice-actor-5");
  render(<Frames />);
  const team = within(screen.getByRole("group", { name: "Team" }));
  const opponents = within(screen.getByRole("group", { name: "Opponents" }));
  expect(team.getByRole("button", { pressed: true }).textContent).toContain("Teammate 1");
  expect(opponents.queryByRole("button", { pressed: true })).toBeNull();
  act(() => engine.set(teamSnapshot(3, 3)));
  expect(team.queryByRole("button", { pressed: true })).toBeNull();
  expect(opponents.getByRole("button", { pressed: true }).textContent).toContain("Enemy Bot 2");
  act(() => engine.set(teamSnapshot(3, null)));
  expect(team.queryByRole("button", { pressed: true })).toBeNull();
  expect(opponents.queryByRole("button", { pressed: true })).toBeNull();
});

it.each([1, 2, 3] as const)(
  "renders team-size %i roster counts without the player in team frames",
  (size) => {
    engine.set(teamSnapshot(size));
    render(<Frames />);
    expect(screen.getAllByRole("region", { name: "opponent status" })).toHaveLength(size);
    if (size === 1) expect(screen.queryByRole("group", { name: "Team" })).toBeNull();
    else {
      const team = within(screen.getByRole("group", { name: "Team" }));
      expect(team.getAllByRole("region", { name: "teammate status" })).toHaveLength(size - 1);
      expect(team.queryByText("Player")).toBeNull();
    }
  },
);

it("targets/focuses every roster slot and cycles only living opponents", () => {
  engine.set(teamSnapshot(3));
  render(<Frames />);
  for (const [name, actor] of [
    ["Teammate 1", 2],
    ["Teammate 2", 4],
    ["Enemy Bot 2", 3],
    ["Enemy Bot 3", 5],
  ] as const) {
    const frame = screen.getByText(name).closest("button")!;
    fireEvent.click(frame);
    expect(input.readInput()[17]).toBe(actor);
    fireEvent.contextMenu(frame);
    expect(focus.get()).toBe(teamSnapshot(3).roster?.[actor].id);
  }
  dispatch("nextTarget");
  expect(input.readInput()[17]).toBe(3);
  engine.set(teamSnapshot(3, 3));
  dispatch("setFocus");
  expect(focus.get()).toBe("practice-actor-3");
  dispatch("nextTarget");
  expect(input.readInput()[17]).toBe(5);
});

it("changes practice mode with a reset and drains the request once", () => {
  engine.set(teamSnapshot(1));
  interaction.set({ ...interaction.get(), panel: "menu" });
  const canvas = document.createElement("canvas");
  document.body.append(canvas);
  const router = installInput(canvas, () => {});
  try {
    render(<GameMenu />);
    fireEvent.change(screen.getByRole("combobox", { name: "Practice mode" }), {
      target: { value: "3" },
    });
    const sample = router.sample();
    expect(sample[5]).toBe(1);
    expect(sample[18]).toBe(3);
    expect(router.sample().slice(17)).toEqual([-1, 0]);
    expect(interaction.get().panel).toBeNull();
    expect(focus.get()).toBeNull();
  } finally {
    router.dispose();
    canvas.remove();
  }
});
