import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { adaptSnapshot } from "../../web/src/bridge/snapshot";
import { withHandles } from "../../web/src/domain/handles";
import type { MatchTicket } from "../../web/src/domain/network";
import { Diagnostics } from "../../web/src/features/diagnostics/Diagnostics";
import { MatchStatus } from "../../web/src/features/match-status/MatchStatus";
import { GameMenu } from "../../web/src/features/menu/GameMenu";
import { Nameplates } from "../../web/src/features/nameplates/Nameplates";
import { Frames } from "../../web/src/features/unit-frames/Frames";
import { diagnosticsVisible } from "../../web/src/runtime/diagnostics";
import { createLink, type Link } from "../../web/src/runtime/network";
import { characterKits } from "../../web/src/state/appearance";
import { networkLink } from "../../web/src/state/network";
import { engine, error, focus, interaction, nameplateScene } from "../../web/src/state/session";
import { humanKit } from "../domain/kit-fixture";
import { snapshot, teamSnapshot } from "./fixtures";

vi.mock("../../web/src/runtime/diagnostics", async (original) => ({
  ...(await original<typeof import("../../web/src/runtime/diagnostics")>()),
  debugEnabled: true,
}));

const ticket: MatchTicket = {
  server: "ws://127.0.0.1:8080/play",
  ticket: "ab01",
  expires: Math.floor(Date.now() / 1000) + 3600,
};

type Fake = {
  readyState: number;
  binaryType: string;
  onopen: (() => void) | null;
  onclose: (() => void) | null;
  onerror: (() => void) | null;
  onmessage: ((event: { data: unknown }) => void) | null;
  send(): void;
  close(): void;
};

/** A real link over fake sockets, timers and storage. */
function fakeLink() {
  const sockets: Fake[] = [];
  const values = new Map<string, string>();
  let pending: (() => void) | null = null;
  const storage = {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => void values.set(key, value),
    removeItem: (key: string) => void values.delete(key),
  };
  const link = createLink({
    createSocket: () => {
      const socket: Fake = {
        readyState: 0,
        binaryType: "blob",
        onopen: null,
        onclose: null,
        onerror: null,
        onmessage: null,
        send() {},
        close() {},
      };
      sockets.push(socket);
      return socket;
    },
    now: () => Date.now(),
    setTimer: (run) => {
      pending = run;
      return 1;
    },
    clearTimer: () => {},
    tab: storage,
    device: storage,
  });
  const latest = () => sockets[sockets.length - 1];
  return {
    link,
    open: () =>
      act(() => {
        latest().readyState = 1;
        latest().onopen?.();
      }),
    drop: () => act(() => latest().onclose?.()),
    /** The reconnect timer fires. */
    retry: () =>
      act(() => {
        const run = pending;
        pending = null;
        run?.();
      }),
    /** The server sends `Error(code)` and closes the socket. */
    closeWith: (code: number) =>
      act(() => {
        latest().onmessage?.({ data: new Uint8Array([3, code]).buffer });
        latest().onclose?.();
      }),
    report: (phase: string, error: string | null = null) =>
      act(() =>
        link.onStatus({
          phase,
          error,
          seat: 1,
          mapDiffers: false,
          rttMs: 82.5,
          inputMs: 140,
          delayMs: 100,
          snapshots: 42,
          underruns: 3,
          hashMismatches: 0,
          predictedTicks: 4,
          corrections: 2,
          lastCorrectionMm: 240,
          queued: 1.2,
          queuedMin: 1,
          pace: 1.012,
          skippedInputs: 0,
          stepsPerSecond: 30,
          sentPerSecond: 29,
          tickRate: 27.52,
        }),
      ),
  };
}

/** A 2v2 network snapshot where enemy 2 (actor 3) and teammate (actor 2) are gone. */
function networkSnapshot() {
  const base = teamSnapshot(2);
  return adaptSnapshot({
    ...base,
    maxHealth: 100,
    nameplates: [
      { tag: "Opponent1", x: 0.4, y: 0.5 },
      { tag: "Opponent2", x: 0.6, y: 0.5 },
      { tag: "Team1", x: 0.5, y: 0.6 },
    ],
    roster: base.roster?.map((unit) => ({
      actor: unit.actor,
      tag: unit.tag,
      health: unit.health,
      connected: unit.actor !== 3 && unit.actor !== 2,
    })),
  });
}

let link: Link;
let fake: ReturnType<typeof fakeLink>;
beforeEach(() => {
  engine.set(null);
  nameplateScene.set(null);
  error.set("");
  focus.set(null);
  interaction.set({ panel: null, quick: false, layout: false, hovered: null, capture: null });
  fake = fakeLink();
  link = fake.link;
  networkLink.set(link);
});
afterEach(() => {
  cleanup();
  networkLink.set(null);
});

it("names network opponents and marks disconnected players, leaving practice unchanged", () => {
  const state = networkSnapshot();
  expect(state.networked).toBe(true);
  expect(state.roster?.map((unit) => [unit.name, !!unit.disconnected])).toEqual([
    ["Player", false],
    ["Opponent 1", false],
    ["Teammate 1", true],
    ["Opponent 2", true],
  ]);
  const practice = teamSnapshot(2);
  expect(practice.networked).toBe(false);
  expect(practice.roster?.[1].name).toBe("Enemy Bot");
  expect(practice.roster?.some((unit) => unit.disconnected)).toBe(false);
  expect(() =>
    adaptSnapshot({
      ...practice,
      maxHealth: 100,
      roster: practice.roster?.map((unit) => ({ ...unit, connected: "yes" })),
    }),
  ).toThrow("Invalid connection flags");
  expect(adaptSnapshot({ ...snapshot(), maxHealth: 100 }).networked).toBe(false);
});

it("shows handles in full on unit frames and by name on nameplates", () => {
  const named = withHandles(networkSnapshot(), ["Rook#0042", "Ash#1000", null, "Guest#0007"]);
  engine.set(named);
  nameplateScene.set(named);
  render(
    <>
      <Frames />
      <Nameplates />
    </>,
  );
  expect(screen.getAllByText("Ash#1000").length).toBeGreaterThan(0);
  expect(screen.getByText("Ash")).toBeTruthy();
  expect(screen.getByText("Teammate 1 · Disconnected")).toBeTruthy();
  expect(screen.getByText("Guest · Disconnected")).toBeTruthy();
});

it("greys out disconnected unit frames and nameplates with a label", () => {
  engine.set(networkSnapshot());
  nameplateScene.set(networkSnapshot());
  render(
    <>
      <Frames />
      <Nameplates />
    </>,
  );
  const opponents = within(screen.getByRole("group", { name: "Opponents" }));
  const frames = opponents.getAllByRole("region", { name: "opponent status" });
  expect(frames[0].className).not.toContain("disconnected");
  expect(frames[1].className).toContain("disconnected");
  expect(within(frames[1]).getByText("Disconnected")).toBeTruthy();
  const team = within(screen.getByRole("group", { name: "Team" }));
  expect(team.getByText("Disconnected")).toBeTruthy();
  const plate = screen.getByRole("button", { name: /Opponent 2 \(disconnected\) health/ });
  expect(plate.className).toContain("disconnected");
  expect(plate.textContent).toContain("Opponent 2 · Disconnected");
  expect(screen.getByRole("button", { name: /^Opponent 1 health/ }).className).not.toContain(
    "disconnected",
  );
});

it("tells the player about connecting, waiting, reconnecting, the end and a rejoin", () => {
  render(<MatchStatus />);
  expect(screen.queryByRole("status")).toBeNull();
  act(() => link.join(ticket));
  expect(screen.getByRole("status").textContent).toContain("Connecting to the match…");
  fake.open();
  expect(screen.getByRole("status").textContent).toContain("Joining the match…");
  fake.report("waiting");
  expect(screen.getByRole("status").textContent).toContain(
    "Waiting for every player to join · you are seat 2…",
  );
  // With a roster, the banner counts who has joined.
  act(() => engine.set(networkSnapshot()));
  expect(screen.getByRole("status").textContent).toContain(
    "Waiting for every player to join (2 of 4 joined) · you are seat 2…",
  );
  fake.report("playing");
  expect(screen.queryByRole("status")).toBeNull();
  fake.drop();
  expect(screen.getByRole("status").textContent).toContain("Connection lost · reconnecting…");
  expect(screen.getByRole("status").className).toContain("warning");
  fake.retry();
  fake.open();
  fake.closeWith(3);
  expect(screen.getByRole("status").textContent).toContain("The match is over.");
  fireEvent.click(screen.getByRole("button", { name: "Dismiss" }));
  // The finished match is forgotten, so nothing is offered.
  expect(screen.queryByRole("status")).toBeNull();
  // Leaving a running match on purpose still offers to rejoin it.
  act(() => link.join(ticket));
  act(() => link.leave());
  expect(screen.getByRole("status").textContent).toContain("A match you joined is still open.");
  fireEvent.click(screen.getByRole("button", { name: "Rejoin match" }));
  expect(link.state.get().phase).toBe("connecting");
  // Losing the seat to another tab offers to take it back here.
  fake.open();
  fake.closeWith(8);
  expect(screen.getByRole("status").textContent).toContain(
    "This seat is now playing in another tab or window.",
  );
  fireEvent.click(screen.getByRole("button", { name: "Play here" }));
  expect(link.state.get().phase).toBe("connecting");
  // Leaving a finished match forgets it, so no stale Rejoin prompt appears.
  fake.open();
  fake.report("finished");
  act(() => link.leave());
  expect(screen.queryByRole("status")).toBeNull();
  expect(link.remembered()).toBeNull();
});

it("offers leaving in the menu during a match and hides practice-only controls", () => {
  characterKits.set([humanKit()]);
  engine.set(snapshot());
  render(<GameMenu />);
  expect(screen.getByText("Local solo practice")).toBeTruthy();
  expect(screen.getByRole("combobox", { name: "Practice mode" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "Customize character" })).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Leave match" })).toBeNull();
  act(() => link.join(ticket));
  expect(screen.getByText("Network match")).toBeTruthy();
  expect(screen.queryByRole("combobox", { name: "Practice mode" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Reset practice" })).toBeNull();
  // Character look is locked once in a match.
  expect(screen.queryByRole("button", { name: "Customize character" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Leave match" }));
  expect(link.state.get().phase).toBe("idle");
  expect(link.read()).toEqual([]);
  // Back in practice, the match can be rejoined from the menu and customization returns.
  expect(screen.getByRole("button", { name: "Customize character" })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Rejoin last match" }));
  expect(link.state.get().phase).toBe("connecting");
  characterKits.set([]);
});

it("shows network statistics in the diagnostics overlay during a match", () => {
  diagnosticsVisible.set(true);
  engine.set(snapshot());
  render(<Diagnostics />);
  expect(screen.queryByLabelText("Network match")).toBeNull();
  act(() => link.join(ticket));
  fake.open();
  fake.report("playing");
  const stats = within(screen.getByLabelText("Network match"));
  expect(stats.getByText("82.5 ms")).toBeTruthy();
  expect(stats.getByText("140.0 ms")).toBeTruthy();
  expect(stats.getByText("playing")).toBeTruthy();
  expect(stats.getByText("3")).toBeTruthy();
  expect(stats.getByText("4 ticks")).toBeTruthy();
  expect(stats.getByText("2 · last 240 mm")).toBeTruthy();
  expect(stats.getByText("1.2 · min 1 · pace 101.2%")).toBeTruthy();
  expect(stats.getByText("29/s · 30 steps/s")).toBeTruthy();
  expect(stats.getByText("27.52/s")).toBeTruthy();
  fake.drop();
  expect(stats.getByText("reconnecting · attempt 1")).toBeTruthy();
});

it("a match joined after another ended starts clean, without the ended match's last frames", () => {
  const fake = fakeLink();
  const { link } = fake;
  act(() => link.join(ticket));
  fake.open();
  expect(link.read()[0]).toBe(1);
  // The server ends the match: `Error(MatchOver)`, then it closes.
  fake.closeWith(3);
  expect(link.state.get()).toMatchObject({ phase: "ended", ended: "MatchOver" });
  expect(link.read()).toEqual([]);
  // The next match (a queue found it): until its socket opens, the engine
  // must see nothing — not the old generation and its `MatchOver` frame,
  // which would end the new match before it starts.
  act(() => link.join({ ...ticket, ticket: "cd02" }));
  expect(link.read()).toEqual([0]);
  fake.open();
  expect(link.read()).toEqual([2]);
  expect(link.state.get().phase).toBe("open");
});

it("retains the time-limit draw outcome in MatchStatus after MatchOver closes the socket", () => {
  render(<MatchStatus />);
  act(() => link.join(ticket));
  fake.open();
  fake.report("playing");
  act(() =>
    engine.set(
      adaptSnapshot({
        ...teamSnapshot(1),
        maxHealth: 100,
        tick: 36_000,
        maxRoundTicks: 36_000,
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
    ),
  );
  // While the match lingers in finished state, MatchStatus stays hidden (RoundClock owns the banner).
  fake.report("finished");
  expect(screen.queryByRole("status")).toBeNull();
  // Server sends Error(MatchOver) and closes; engine resets to local practice.
  fake.closeWith(3);
  act(() =>
    engine.set(snapshot({ tick: 0, maxRoundTicks: 36_000, tickRate: 30, roundResult: null })),
  );
  const banner = screen.getByRole("status");
  expect(banner.textContent).toContain("Time's up · draw · The match is over.");
  expect(banner.className).toContain("warning");
  // Dismissing clears the remembered outcome for subsequent matches.
  fireEvent.click(screen.getByRole("button", { name: "Dismiss" }));
  expect(screen.queryByRole("status")).toBeNull();
  act(() => link.join(ticket));
  fake.open();
  fake.closeWith(3);
  expect(screen.getByRole("status").textContent).toContain("The match is over.");
  expect(screen.getByRole("status").textContent).not.toContain("Time's up");
});

it("preserves MatchOver outcome across batched/reconnecting closes and never leaks across fast rematches", () => {
  const finishedNetSnap = () =>
    adaptSnapshot({
      ...teamSnapshot(1),
      maxHealth: 100,
      tick: 36_000,
      maxRoundTicks: 36_000,
      tickRate: 30,
      won: true,
      roundResult: "time_limit_draw",
      roster: teamSnapshot(1).roster?.map((unit) => ({
        actor: unit.actor,
        tag: unit.tag,
        health: unit.health,
        connected: true,
      })),
    });

  render(<MatchStatus />);

  // 1) Batched finished snapshot + MatchOver in one render pass before any open-phase effect runs,
  //    then engine resets to practice.
  act(() => link.join(ticket));
  fake.open();
  fake.report("playing");
  act(() => {
    engine.set(finishedNetSnap());
    fake.closeWith(3);
  });
  act(() =>
    engine.set(snapshot({ tick: 0, maxRoundTicks: 36_000, tickRate: 30, roundResult: null })),
  );
  expect(screen.getByRole("status").textContent).toContain("Time's up · draw · The match is over.");

  // 2) Reconnecting while final snapshot drains, engine resets to practice, then MatchOver closes.
  act(() => link.join(ticket));
  fake.open();
  fake.report("playing");
  fake.drop();
  act(() => engine.set(finishedNetSnap()));
  act(() =>
    engine.set(snapshot({ tick: 0, maxRoundTicks: 36_000, tickRate: 30, roundResult: null })),
  );
  fake.report("closed", "MatchOver");
  expect(screen.getByRole("status").textContent).toContain("Time's up · draw · The match is over.");

  // 3) Fast rematch: engine still carries prior match's finished snapshot while new socket opens
  //    (before Bevy processes Welcome / reports status). Prior outcome must NOT leak.
  act(() => engine.set(finishedNetSnap()));
  act(() => link.join({ ...ticket, ticket: "cd02" }));
  fake.open();
  fake.closeWith(3);
  expect(screen.getByRole("status").textContent).toContain("The match is over.");
  expect(screen.getByRole("status").textContent).not.toContain("Time's up");
});
