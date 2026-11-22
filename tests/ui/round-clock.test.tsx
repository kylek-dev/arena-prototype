import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it } from "vitest";
import { adaptSnapshot } from "../../web/src/bridge/snapshot";
import { IDLE_QUEUE } from "../../web/src/domain/matchmaking";
import { App } from "../../web/src/app/App";
import { CombatErrors } from "../../web/src/features/combat-errors/CombatErrors";
import { MatchStatus } from "../../web/src/features/match-status/MatchStatus";
import { RoundClock } from "../../web/src/features/round-clock/RoundClock";
import { input } from "../../web/src/runtime/commands";
import type { Queue } from "../../web/src/runtime/matchmaking";
import type { Link } from "../../web/src/runtime/network";
import { beforeMatch, customizerVisible } from "../../web/src/state/appearance";
import { matchQueue, networkLink } from "../../web/src/state/network";
import { engine, error, focus, interaction } from "../../web/src/state/session";
import { createStore } from "../../web/src/state/store";
import { snapshot, teamSnapshot } from "./fixtures";

const MAX_TICKS = 36_000;

beforeEach(() => {
  input.cancel();
  engine.set(null);
  error.set("");
  focus.set(null);
  matchQueue.set(null);
  networkLink.set(null);
  customizerVisible.set(false);
  beforeMatch.set(false);
  interaction.set({ panel: null, quick: false, layout: false, hovered: null, capture: null });
});

afterEach(() => {
  cleanup();
  matchQueue.set(null);
  networkLink.set(null);
});

it("renders nothing when the engine is absent or lacks round-clock metadata", () => {
  const { container } = render(<RoundClock />);
  expect(container.firstChild).toBeNull();
  act(() => engine.set(snapshot()));
  expect(screen.queryByRole("timer")).toBeNull();
  expect(screen.queryByRole("status")).toBeNull();
});

it("displays the authoritative MM:SS countdown, warning minute, and expired state", () => {
  engine.set(snapshot({ tick: 0, maxRoundTicks: MAX_TICKS, tickRate: 30, roundResult: null }));
  render(<RoundClock />);
  const timer = screen.getByRole("timer", { name: "Round time remaining" });
  expect(timer.getAttribute("aria-live")).toBe("off");
  expect(timer.textContent).toBe("20:00");
  expect(timer.className).not.toContain("warning");
  expect(timer.className).not.toContain("expired");

  act(() =>
    engine.set(snapshot({ tick: 30, maxRoundTicks: MAX_TICKS, tickRate: 30, roundResult: null })),
  );
  expect(timer.textContent).toBe("19:59");

  // 61 seconds remaining: still normal.
  act(() =>
    engine.set(
      snapshot({
        tick: MAX_TICKS - 61 * 30,
        maxRoundTicks: MAX_TICKS,
        tickRate: 30,
        roundResult: null,
      }),
    ),
  );
  expect(timer.textContent).toBe("01:01");
  expect(timer.className).not.toContain("warning");

  // 60 seconds remaining: enters final warning minute.
  act(() =>
    engine.set(
      snapshot({
        tick: MAX_TICKS - 60 * 30,
        maxRoundTicks: MAX_TICKS,
        tickRate: 30,
        roundResult: null,
      }),
    ),
  );
  expect(timer.textContent).toBe("01:00");
  expect(timer.className).toContain("warning");

  // Winning inside the final minute clears the warning highlight.
  act(() =>
    engine.set(
      snapshot({
        tick: MAX_TICKS - 30 * 30,
        maxRoundTicks: MAX_TICKS,
        tickRate: 30,
        won: true,
        roundResult: "won",
      }),
    ),
  );
  expect(timer.textContent).toBe("00:30");
  expect(timer.className).not.toContain("warning");
  expect(timer.className).not.toContain("expired");

  // Hitting the round limit shows 00:00 and expired styling.
  act(() =>
    engine.set(
      snapshot({
        tick: MAX_TICKS,
        maxRoundTicks: MAX_TICKS,
        tickRate: 30,
        won: true,
        roundResult: "time_limit_draw",
      }),
    ),
  );
  expect(timer.textContent).toBe("00:00");
  expect(timer.className).not.toContain("warning");
  expect(timer.className).toContain("expired");
});

it("renders the Time's up · draw banner in local practice with a working Reset practice control", () => {
  engine.set(
    snapshot({
      tick: MAX_TICKS,
      maxRoundTicks: MAX_TICKS,
      tickRate: 30,
      won: true,
      roundResult: "time_limit_draw",
    }),
  );
  render(<RoundClock />);
  const banner = screen.getByRole("status");
  expect(banner.className).toContain("time-limit");
  expect(banner.textContent).toContain("Time's up · draw");
  expect(banner.textContent).toContain("Round time limit reached");
  const reset = screen.getByRole("button", { name: "Reset practice" });
  fireEvent.click(reset);
  expect(input.readInput()[5]).toBe(1);
  // Once the engine resets, the banner disappears and the clock returns to 20:00.
  act(() =>
    engine.set(
      snapshot({
        tick: 0,
        maxRoundTicks: MAX_TICKS,
        tickRate: 30,
        won: false,
        roundResult: null,
      }),
    ),
  );
  expect(screen.queryByRole("status")).toBeNull();
  expect(screen.getByRole("timer").textContent).toBe("20:00");
});

it("renders networked match outcomes without a Reset practice button", () => {
  const netSnap = (roundResult: "won" | "lost" | "draw" | "time_limit_draw") =>
    adaptSnapshot({
      ...teamSnapshot(1),
      maxHealth: 100,
      tick: roundResult === "time_limit_draw" ? 1800 : 450,
      maxRoundTicks: 1800,
      tickRate: 30,
      won: true,
      roundResult,
      roster: teamSnapshot(1).roster?.map((unit) => ({
        actor: unit.actor,
        tag: unit.tag,
        health: unit.health,
        connected: true,
      })),
    });

  engine.set(netSnap("time_limit_draw"));
  render(<RoundClock />);
  expect(screen.getByRole("timer").textContent).toBe("00:00");
  expect(screen.getByRole("status").textContent).toContain("Time's up · draw");
  expect(screen.getByRole("status").textContent).toContain(
    "Round time limit reached · match closing shortly",
  );
  expect(screen.queryByRole("button", { name: "Reset practice" })).toBeNull();

  act(() => engine.set(netSnap("won")));
  expect(screen.getByRole("status").className).toContain("victory");
  expect(screen.getByRole("status").textContent).toContain("Victory");
  expect(screen.getByRole("status").textContent).toContain("Match closing shortly");

  act(() => engine.set(netSnap("lost")));
  expect(screen.getByRole("status").className).toContain("defeat");
  expect(screen.getByRole("status").textContent).toContain("Defeat");

  act(() => engine.set(netSnap("draw")));
  expect(screen.getByRole("status").className).toContain("draw");
  expect(screen.getByRole("status").textContent).toContain("Draw");
});

it("shifts clock, result, combat errors, and match status below the QueueBar when queued", () => {
  const queueState = createStore(IDLE_QUEUE);
  matchQueue.set({ state: queueState } as unknown as Queue);
  const linkState = createStore({
    phase: "connecting" as const,
    attempt: 0,
    ended: null,
    status: null,
  });
  networkLink.set({
    state: linkState,
    remembered: () => null,
    join: () => {},
    leave: () => {},
  } as unknown as Link);
  engine.set(snapshot({ tick: 60, maxRoundTicks: MAX_TICKS, tickRate: 30, roundResult: null }));
  const { container } = render(
    <>
      <RoundClock />
      <CombatErrors />
      <MatchStatus />
    </>,
  );
  const timer = screen.getByRole("timer");
  const errors = screen.getByRole("status", { name: "Combat errors" });
  const status = container.querySelector(".match-status")!;
  expect(timer.className).not.toContain("below-queue");
  expect(errors.className).not.toContain("below-queue");
  expect(status.className).not.toContain("below-queue");
  act(() => queueState.set({ ...IDLE_QUEUE, connection: "online", phase: "queued", mode: "2v2" }));
  expect(timer.className).toContain("below-queue");
  expect(errors.className).toContain("below-queue");
  expect(status.className).toContain("below-queue");
  act(() => queueState.set(IDLE_QUEUE));
  expect(timer.className).not.toContain("below-queue");
  expect(errors.className).not.toContain("below-queue");
  expect(status.className).not.toContain("below-queue");
});

it("suppresses the round result banner whenever MatchStatus owns the banner slot", () => {
  const linkState = createStore<{
    phase: "idle" | "connecting" | "open" | "reconnecting" | "ended";
    attempt: number;
    ended: string | null;
    status: { phase: "connecting" | "waiting" | "playing" | "finished" | "closed" } | null;
  }>({
    phase: "open",
    attempt: 0,
    ended: null,
    status: { phase: "finished" },
  });
  networkLink.set({
    state: linkState,
    remembered: () => null,
    join: () => {},
    leave: () => {},
  } as unknown as Link);
  engine.set(
    adaptSnapshot({
      ...teamSnapshot(1),
      maxHealth: 100,
      tick: MAX_TICKS,
      maxRoundTicks: MAX_TICKS,
      tickRate: 30,
      won: true,
      roundResult: "time_limit_draw",
      roster: teamSnapshot(1).roster?.map((unit) => ({
        actor: unit.actor,
        tag: unit.tag,
        health: unit.health,
        connected: true,
      })),
    }),
  );
  render(
    <>
      <RoundClock />
      <MatchStatus />
    </>,
  );
  // During the 5s finished linger, RoundClock renders the banner and MatchStatus is absent.
  expect(screen.getAllByRole("status")).toHaveLength(1);
  expect(screen.getByRole("status").textContent).toContain("Round time limit reached");

  // Once MatchOver closes the socket — even before engine resets to local practice —
  // RoundClock yields immediately so only MatchStatus renders (no duplicate/contradictory banner).
  act(() =>
    linkState.set({
      phase: "ended",
      attempt: 0,
      ended: "MatchOver",
      status: { phase: "finished" },
    }),
  );
  expect(screen.getAllByRole("status")).toHaveLength(1);
  expect(screen.getByRole("status").textContent).toContain("Time's up · draw · The match is over.");
  expect(screen.getByRole("status").textContent).not.toContain("match closing shortly");
});

it("hides during exclusive UI modes and fatal errors, but stays visible with the Spellbook", () => {
  engine.set(
    snapshot({
      tick: MAX_TICKS,
      maxRoundTicks: MAX_TICKS,
      tickRate: 30,
      won: true,
      roundResult: "time_limit_draw",
    }),
  );
  render(<RoundClock />);
  expect(screen.getByRole("timer")).toBeTruthy();
  expect(screen.getByRole("status")).toBeTruthy();

  for (const mode of [
    { layout: true },
    { quick: true },
    { capture: { action: "menu", entry: 0 as const } },
    { panel: "menu" as const },
    { panel: "settings" as const },
  ]) {
    act(() =>
      interaction.set({
        panel: null,
        quick: false,
        layout: false,
        hovered: null,
        capture: null,
        ...mode,
      }),
    );
    expect(screen.queryByRole("timer")).toBeNull();
    expect(screen.queryByRole("status")).toBeNull();
  }

  // Non-modal Spellbook keeps the combat HUD and round clock visible.
  act(() =>
    interaction.set({
      panel: "spellbook",
      quick: false,
      layout: false,
      hovered: null,
      capture: null,
    }),
  );
  expect(screen.getByRole("timer")).toBeTruthy();
  expect(screen.getByRole("status")).toBeTruthy();

  // Fatal engine error suppresses the clock.
  act(() => error.set("Engine failed"));
  expect(screen.queryByRole("timer")).toBeNull();
});

it("mounts in App during arena play and hides on the start screen and customizer", () => {
  engine.set(snapshot({ tick: 90, maxRoundTicks: MAX_TICKS, tickRate: 30, roundResult: null }));
  render(<App />);
  expect(screen.getByRole("timer", { name: "Round time remaining" }).textContent).toBe("19:57");
  act(() => beforeMatch.set(true));
  expect(screen.queryByRole("timer", { name: "Round time remaining" })).toBeNull();
  act(() => {
    beforeMatch.set(false);
    customizerVisible.set(true);
  });
  expect(screen.queryByRole("timer", { name: "Round time remaining" })).toBeNull();
});
