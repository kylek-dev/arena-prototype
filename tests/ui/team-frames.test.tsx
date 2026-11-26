import { matchQueue } from "../../web/src/state/network";
import { adaptSnapshot } from "../../web/src/bridge/snapshot";
import { withHandles } from "../../web/src/domain/handles";
import { fakeQueue } from "./fake-queue";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it } from "vitest";
import { Frames } from "../../web/src/features/unit-frames/Frames";
import { PlayWindow } from "../../web/src/features/launcher/PlayWindow";
import { openPlay } from "../../web/src/state/windows";
import { TooltipProvider } from "../../web/src/ui/Tooltip";
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
afterEach(() => {
  cleanup();
  matchQueue.set(null);
});

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
  // From the launcher's Play window, with no menu open.
  interaction.set({ ...interaction.get(), panel: null });
  const canvas = document.createElement("canvas");
  document.body.append(canvas);
  const router = installInput(canvas, () => {});
  try {
    openPlay("practice");
    render(
      <TooltipProvider>
        <PlayWindow />
      </TooltipProvider>,
    );
    fireEvent.change(screen.getByRole("combobox", { name: "Practice mode" }), {
      target: { value: "3" },
    });
    const sample = router.sample();
    expect(sample[5]).toBe(1);
    expect(sample[18]).toBe(3);
    expect(router.sample().slice(17)).toEqual([-1, 0, 0, -2, 0]);
    expect(interaction.get().panel).toBeNull();
    expect(focus.get()).toBeNull();
  } finally {
    router.dispose();
    canvas.remove();
  }
});

it("shows party members with the team, greyed out when they are not in this match", () => {
  const fake = fakeQueue();
  matchQueue.set(fake.queue);
  fake.queue.connect();
  fake.open();
  fake.say({
    type: "party",
    code: "K7P2QX",
    members: 3,
    leader: true,
    queue: null,
    mates: ["Ash#1000", "Bea#0002"],
  });
  // Practice: nobody from the party is here.
  engine.set(teamSnapshot(1));
  const { rerender } = render(<Frames />);
  let team = screen.getByRole("group", { name: "Team" });
  expect([...team.querySelectorAll(".party-away")].map((f) => f.textContent)).toEqual([
    "Ash#1000Not in this match",
    "Bea#0002Not in this match",
  ]);
  // A network match with Ash on the team: Ash is a live teammate, Bea stays greyed.
  const base = teamSnapshot(2);
  const network = adaptSnapshot({
    ...base,
    maxHealth: 100,
    roster: base.roster?.map((unit) => ({
      actor: unit.actor,
      tag: unit.tag,
      health: unit.health,
      connected: true,
    })),
  });
  act(() => engine.set(withHandles(network, ["Rook#0042", "Zed#0001", "Ash#1000", "Yan#0002"])));
  rerender(<Frames />);
  team = screen.getByRole("group", { name: "Team" });
  expect(within(team).getByRole("button", { name: /Ash#1000/ })).toBeTruthy();
  expect([...team.querySelectorAll(".party-away")].map((f) => f.textContent)).toEqual([
    "Bea#0002Not in this match",
  ]);
});
