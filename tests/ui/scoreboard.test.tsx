import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { adaptSnapshot, type Snapshot } from "../../web/src/bridge/snapshot";
import { IDLE_QUEUE, type QueueMode, type QueueState } from "../../web/src/domain/matchmaking";
import type { LinkState } from "../../web/src/domain/network";
import type { Kit } from "../../web/src/domain/appearance";
import type { ScoreboardEntry } from "../../web/src/domain/scoreboard";
import { App } from "../../web/src/app/App";
import { MatchStatus } from "../../web/src/features/match-status/MatchStatus";
import { RoundClock } from "../../web/src/features/round-clock/RoundClock";
import { Scoreboard } from "../../web/src/features/scoreboard/Scoreboard";
import type { Queue } from "../../web/src/runtime/matchmaking";
import type { Link } from "../../web/src/runtime/network";
import { beforeMatch, characterKits, customizerVisible } from "../../web/src/state/appearance";
import { matchQueue, networkLink } from "../../web/src/state/network";
import { clearPostMatchScoreboard } from "../../web/src/state/scoreboard";
import { installInput } from "../../web/src/runtime/input-router";
import { input } from "../../web/src/runtime/commands";
import {
  blocksGameplay,
  engine,
  error,
  focus,
  interaction,
  scoreboardVisible,
  setPanel,
} from "../../web/src/state/session";
import { createStore } from "../../web/src/state/store";
import { snapshot, teamSnapshot } from "./fixtures";

function sample2v2Entries(): ScoreboardEntry[] {
  return [
    {
      actor: 0,
      tag: "Player",
      damageDone: 150,
      damageTaken: 60,
      healingDone: 30,
      healingReceived: 45,
      killingBlows: 1,
    },
    {
      actor: 1,
      tag: "Opponent1",
      damageDone: 90,
      damageTaken: 180,
      healingDone: 0,
      healingReceived: 25,
      killingBlows: 0,
    },
    {
      actor: 2,
      tag: "Team1",
      damageDone: 150,
      damageTaken: 85,
      healingDone: 60,
      healingReceived: 15,
      killingBlows: 1,
    },
    {
      actor: 3,
      tag: "Opponent2",
      damageDone: 55,
      damageTaken: 120,
      healingDone: 25,
      healingReceived: 0,
      killingBlows: 0,
    },
  ];
}

function finishedNet2v2(
  roundResult: "won" | "lost" | "draw" | "time_limit_draw" = "won",
  disconnectedActors: number[] = [3],
): Snapshot {
  const base = teamSnapshot(2);
  return adaptSnapshot({
    ...base,
    maxHealth: 100,
    tick: 3675,
    maxRoundTicks: 36_000,
    tickRate: 30,
    won: true,
    roundResult,
    scoreboard: sample2v2Entries(),
    roster: base.roster?.map((unit) => ({
      actor: unit.actor,
      tag: unit.tag,
      health: unit.health,
      connected: !disconnectedActors.includes(unit.actor),
    })),
  });
}

function makeFakeLink(initial?: Partial<LinkState>) {
  const state = createStore<LinkState>({
    phase: "open",
    attempt: 0,
    ended: null,
    status: {
      phase: "finished",
      error: null,
      seat: 0,
      mapDiffers: false,
      rttMs: 40,
      inputMs: 60,
      delayMs: 80,
      snapshots: 100,
      underruns: 0,
      hashMismatches: 0,
      predictedTicks: 1,
      corrections: 0,
      lastCorrectionMm: null,
      queued: 1,
      queuedMin: 1,
      pace: 1,
      skippedInputs: 0,
      stepsPerSecond: 30,
      sentPerSecond: 30,
      tickRate: 30,
    },
    ...initial,
  });
  const leave = vi.fn(() => {
    state.set({ phase: "idle", attempt: 0, ended: null, status: null });
  });
  const link = {
    state,
    leave,
    join: vi.fn(),
    remembered: () => null,
  } as unknown as Link;
  return { link, state, leave };
}

function makeFakeQueue(initial?: Partial<QueueState>) {
  const state = createStore<QueueState>({ ...IDLE_QUEUE, ...initial });
  const queueFn = vi.fn((mode: QueueMode) => {
    state.set({ ...state.get(), connection: "online", phase: "queued", mode, since: 1000 });
  });
  const queue = {
    state,
    queue: queueFn,
    cancel: vi.fn(),
    now: () => 2000,
  } as unknown as Queue;
  return { queue, state, queueFn };
}

beforeEach(() => {
  clearPostMatchScoreboard();
  engine.set(null);
  error.set("");
  focus.set(null);
  matchQueue.set(null);
  networkLink.set(null);
  characterKits.set([]);
  customizerVisible.set(false);
  beforeMatch.set(false);
  interaction.set({ panel: null, quick: false, layout: false, hovered: null, capture: null });
});

afterEach(() => {
  cleanup();
  clearPostMatchScoreboard();
  matchQueue.set(null);
  networkLink.set(null);
  characterKits.set([]);
});

it("stays hidden during local practice and live networked matches", () => {
  engine.set(snapshot({ won: true, roundResult: "won" }));
  const { container } = render(<Scoreboard />);
  expect(container.firstChild).toBeNull();
  expect(blocksGameplay()).toBe(false);

  const liveNet = adaptSnapshot({
    ...teamSnapshot(1),
    maxHealth: 100,
    tick: 120,
    won: false,
    roundResult: null,
    scoreboard: null,
    roster: teamSnapshot(1).roster?.map((u) => ({
      actor: u.actor,
      tag: u.tag,
      health: u.health,
      connected: true,
    })),
  });
  act(() => engine.set(liveNet));
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(scoreboardVisible.get()).toBe(false);
});

it("renders grouped team stats, highlights local player, replaces banners, and stays up after MatchOver", () => {
  const { link, state: linkState } = makeFakeLink();
  const { queue } = makeFakeQueue();
  networkLink.set(link);
  matchQueue.set(queue);

  render(
    <>
      <RoundClock />
      <MatchStatus />
      <Scoreboard />
    </>,
  );

  act(() => engine.set(finishedNet2v2("won", [3])));

  const dialog = screen.getByRole("dialog", { name: "Victory" });
  expect(dialog.getAttribute("data-keyboard-owner")).toBe("true");
  expect(blocksGameplay()).toBe(true);
  expect(dialog.textContent).toContain("2v2 · Duration 02:02 · Match closing shortly");
  // Replaces both the RoundClock "match closing shortly" banner and MatchStatus banner.
  expect(screen.queryByRole("timer")).toBeNull();
  expect(document.querySelector(".round-result")).toBeNull();
  expect(document.querySelector(".match-status")).toBeNull();

  const allies = within(dialog).getByRole("rowgroup", { name: "Your team" });
  const allyRows = within(allies).getAllByRole("row");
  // Heading + 2 sorted player rows (Teammate 1 ties 150 dmg, wins tie-break on healing 60 > 30) + totals.
  expect(allyRows).toHaveLength(4);
  expect(allyRows[1].textContent).toContain("Teammate 1");
  expect(allyRows[2].textContent).toContain("Player");
  expect(allyRows[2].className).toContain("local");
  expect(within(allyRows[2]).getByText("You")).toBeTruthy();
  expect(allyRows[3].textContent).toContain("Your team total");
  expect(allyRows[3].textContent).toContain("300");

  const opponents = within(dialog).getByRole("rowgroup", { name: "Opponents" });
  const oppRows = within(opponents).getAllByRole("row");
  expect(oppRows).toHaveLength(4);
  expect(oppRows[1].textContent).toContain("Opponent 1");
  expect(oppRows[2].textContent).toContain("Opponent 2");
  expect(oppRows[2].className).toContain("disconnected");
  expect(within(oppRows[2]).getByText("Disconnected")).toBeTruthy();

  // Server closes with MatchOver and Bevy resets back to local practice: scoreboard MUST remain open.
  act(() => {
    linkState.set({
      phase: "ended",
      attempt: 0,
      ended: "MatchOver",
      status: linkState.get().status,
    });
    engine.set(snapshot({ tick: 0, won: false, roundResult: null }));
  });
  expect(screen.getByRole("dialog", { name: "Victory" }).textContent).toContain("Match ended");
  expect(document.querySelector(".match-status")).toBeNull();
});

it("sorts columns descending then ascending per team with aria-sort feedback", () => {
  engine.set(finishedNet2v2("time_limit_draw", []));
  render(<Scoreboard />);

  const dialog = screen.getByRole("dialog", { name: "Time's up · draw" });
  const dmgHeader = within(dialog).getByRole("columnheader", { name: /Damage Done/ });
  expect(dmgHeader.getAttribute("aria-sort")).toBe("descending");

  // Click Damage Taken -> sorts descending (Team1 85 before Player 60).
  const takenBtn = within(dialog).getByRole("button", { name: "Sort by Damage Taken" });
  fireEvent.click(takenBtn);
  const takenHeader = within(dialog).getByRole("columnheader", { name: /Damage Taken/ });
  expect(takenHeader.getAttribute("aria-sort")).toBe("descending");
  expect(dmgHeader.getAttribute("aria-sort")).toBe("none");

  const allies = within(dialog).getByRole("rowgroup", { name: "Your team" });
  expect(within(allies).getAllByRole("row")[1].textContent).toContain("Teammate 1");

  // Click Damage Taken again -> flips to ascending (Player 60 before Team1 85).
  fireEvent.click(takenBtn);
  expect(takenHeader.getAttribute("aria-sort")).toBe("ascending");
  expect(within(allies).getAllByRole("row")[1].textContent).toContain("Player");
});

it("queues again for the same mode, respects party-leader limits, and clears on new match", () => {
  const { link, state: linkState, leave } = makeFakeLink();
  const {
    queue,
    state: queueState,
    queueFn,
  } = makeFakeQueue({
    connection: "online",
    party: { code: "ABC234", members: 2, leader: false, mates: [] },
  });
  networkLink.set(link);
  matchQueue.set(queue);
  engine.set(finishedNet2v2("lost"));

  render(<Scoreboard />);
  const queueBtn = screen.getByRole("button", { name: "Queue again" }) as HTMLButtonElement;
  expect(queueBtn.disabled).toBe(true);
  expect(screen.getByRole("status").textContent).toContain(
    "Only the party leader can start a search.",
  );

  // Promote to party leader: Queue again becomes enabled and queues the party for 2v2.
  act(() =>
    queueState.set({
      ...queueState.get(),
      party: { code: "ABC234", members: 2, leader: true, mates: [] },
    }),
  );
  expect(queueBtn.disabled).toBe(false);
  fireEvent.click(queueBtn);
  expect(leave).toHaveBeenCalledTimes(1);
  expect(queueFn).toHaveBeenCalledWith("2v2");
  // With the scoreboard open the search keeps the top bar (the center is
  // taken), and the scoreboard moves below it.
  expect(document.querySelector(".scoreboard-backdrop")?.className).toContain("below-queue");
  expect((screen.getByRole("button", { name: "In 2v2 queue" }) as HTMLButtonElement).disabled).toBe(
    true,
  );

  // Joining a new match clears the board and prevents stale finished snapshot leakage across
  // connecting -> open (status null) -> waiting -> playing transitions.
  act(() => linkState.set({ phase: "connecting", attempt: 0, ended: null, status: null }));
  expect(screen.queryByRole("dialog")).toBeNull();
  act(() => linkState.set({ phase: "open", attempt: 0, ended: null, status: null }));
  expect(screen.queryByRole("dialog")).toBeNull();
  act(() =>
    linkState.set({
      phase: "open",
      attempt: 0,
      ended: null,
      status: { ...makeFakeLink().state.get().status!, phase: "waiting" },
    }),
  );
  expect(screen.queryByRole("dialog")).toBeNull();
  act(() =>
    linkState.set({
      phase: "open",
      attempt: 0,
      ended: null,
      status: { ...makeFakeLink().state.get().status!, phase: "playing" },
    }),
  );
  expect(screen.queryByRole("dialog")).toBeNull();
});

it("prefers the finished match mode over stale queueState.mode and guards in-flight clicks & refusals", () => {
  const { link } = makeFakeLink();
  const queueState = createStore<QueueState>({
    ...IDLE_QUEUE,
    connection: "online",
    mode: "1v1", // Stale from an earlier search; finished match is 2v2!
    party: { code: "ABC234", members: 2, leader: true, mates: [] },
  });
  const queueFn = vi.fn(); // Deliberately async (leaves phase idle until door replies).
  const queue = {
    state: queueState,
    queue: queueFn,
    cancel: vi.fn(),
    now: () => 2000,
  } as unknown as Queue;
  networkLink.set(link);
  matchQueue.set(queue);
  engine.set(finishedNet2v2("won"));

  render(<Scoreboard />);
  const queueBtn = screen.getByRole("button", { name: "Queue again" }) as HTMLButtonElement;
  expect(queueBtn.disabled).toBe(false);
  fireEvent.click(queueBtn);
  expect(queueFn).toHaveBeenCalledTimes(1);
  expect(queueFn).toHaveBeenCalledWith("2v2");

  // While the request is in flight, button switches to disabled "Queueing…" to prevent double-clicks.
  const pendingBtn = screen.getByRole("button", { name: "Queueing…" }) as HTMLButtonElement;
  expect(pendingBtn.disabled).toBe(true);
  fireEvent.click(pendingBtn);
  expect(queueFn).toHaveBeenCalledTimes(1);

  // Server refusal re-enables the button and displays the refusal notice.
  act(() => queueState.set({ ...queueState.get(), notice: "party_busy" }));
  expect(screen.getByRole("status").textContent).toContain("This party is currently in a queue.");
  expect((screen.getByRole("button", { name: "Queue again" }) as HTMLButtonElement).disabled).toBe(
    false,
  );

  // Oversized party (3 players after a 2v2 match) blocks queueing again.
  act(() =>
    queueState.set({
      ...queueState.get(),
      notice: null,
      party: { code: "ABC234", members: 3, leader: true, mates: [] },
    }),
  );
  expect((screen.getByRole("button", { name: "Queue again" }) as HTMLButtonElement).disabled).toBe(
    true,
  );
  expect(screen.getByRole("status").textContent).toContain(
    "This queue accepts parties of up to 2 players.",
  );
});

it("closes open combat modals/editors on scoreboard open, stays dismissed during linger, and isolates keyboard input", () => {
  const { link } = makeFakeLink();
  networkLink.set(link);
  setPanel("menu");
  customizerVisible.set(true);

  const canvas = document.createElement("canvas");
  canvas.tabIndex = 0;
  document.body.appendChild(canvas);
  const router = installInput(canvas, () => {});
  try {
    render(<Scoreboard />);
    act(() => engine.set(finishedNet2v2("won")));

    expect(screen.getByRole("dialog", { name: "Victory" })).toBeTruthy();
    expect(interaction.get().panel).toBeNull();
    expect(customizerVisible.get()).toBe(false);
    expect(blocksGameplay()).toBe(true);

    // Gameplay key presses are blocked while the scoreboard is open.
    fireEvent.keyDown(window, { code: "KeyW" });
    expect(router.sample()[1]).toBe(0);

    // Clicking Leave during the 5 s finish linger stays dismissed even if another finished snapshot arrives.
    fireEvent.click(screen.getByRole("button", { name: "Leave" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    act(() => engine.set(finishedNet2v2("won")));
    expect(screen.queryByRole("dialog")).toBeNull();

    // Once engine transitions back to practice (or a new match), capture rearms cleanly.
    act(() => engine.set(snapshot({ tick: 0, won: false, roundResult: null })));
    act(() => {
      link.state.set(makeFakeLink().state.get());
      engine.set(finishedNet2v2("lost"));
    });
    expect(screen.getByRole("dialog", { name: "Defeat" })).toBeTruthy();
  } finally {
    router.dispose();
    canvas.remove();
    input.cancel();
  }
});

it("returns to the home screen on Leave when character kits exist and leaves the match link", () => {
  const { link, leave } = makeFakeLink();
  networkLink.set(link);
  characterKits.set([{ id: "female", label: "Female" } as unknown as Kit]);
  engine.set(finishedNet2v2("draw"));

  render(<Scoreboard />);
  expect(screen.getByRole("dialog", { name: "Draw" })).toBeTruthy();

  fireEvent.click(screen.getByRole("button", { name: "Leave" }));
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(blocksGameplay()).toBe(true); // prematch home screen is now active
  expect(beforeMatch.get()).toBe(true);
  expect(leave).toHaveBeenCalledTimes(1);
});

it("stays out of blank prematch when no character kits are installed and hides on fatal errors", () => {
  engine.set(finishedNet2v2("won"));
  render(<App />);
  expect(screen.getByRole("dialog", { name: "Victory" })).toBeTruthy();

  act(() => error.set("WebGL lost"));
  expect(screen.queryByRole("dialog")).toBeNull();

  act(() => error.set(""));
  fireEvent.click(screen.getByRole("button", { name: "Leave" }));
  expect(beforeMatch.get()).toBe(false);
  expect(blocksGameplay()).toBe(false);
});
