import { EMPTY_CHAT } from "../../web/src/domain/chat";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { App } from "../../web/src/app/App";
import { adaptSnapshot } from "../../web/src/bridge/snapshot";
import { IDLE_QUEUE } from "../../web/src/domain/matchmaking";
import type { LinkState } from "../../web/src/domain/network";
import {
  activateSlot,
  dispatch,
  input,
  interactEntity,
  interactUnit,
} from "../../web/src/runtime/commands";
import { installInput } from "../../web/src/runtime/input-router";
import { beforeMatch, customizerVisible } from "../../web/src/state/appearance";
import { matchQueue, networkLink } from "../../web/src/state/network";
import { clearPostMatchScoreboard } from "../../web/src/state/scoreboard";
import {
  customLobbyVisible,
  engine,
  error,
  focus,
  interaction,
  nameplateScene,
  setPanel,
} from "../../web/src/state/session";
import { createStore } from "../../web/src/state/store";
import { teamSnapshot } from "./fixtures";

function spectatorSnapshot(
  opts: {
    follow?: number | null;
    won?: boolean;
    roundResult?: "won" | "lost" | "draw" | null;
  } = {},
) {
  const base = teamSnapshot(2, null);
  const won = opts.won ?? false;
  return adaptSnapshot({
    ...base,
    maxHealth: 100,
    maxResource: 100,
    tick: 450,
    maxRoundTicks: 9000,
    tickRate: 30,
    selected: false,
    selectedActor: null,
    selectedPlayer: false,
    spectating: true,
    spectatorFollow: opts.follow ?? null,
    won,
    roundResult: opts.roundResult ?? null,
    nameplates: [
      { tag: "Player", x: 0.25, y: 0.4 },
      { tag: "Opponent1", x: 0.75, y: 0.4 },
    ],
    roster: [
      {
        actor: 0,
        tag: "Player",
        health: 80,
        resource: 65,
        connected: true,
        cast: { spell: "rift_bolt", remainingTicks: 30, totalTicks: 45 },
      },
      { actor: 1, tag: "Opponent1", health: 90, resource: 100, connected: true },
      { actor: 2, tag: "Team1", health: 100, resource: 40, lockoutRemaining: 30, connected: true },
      { actor: 3, tag: "Opponent2", health: 50, resource: 0, connected: false },
    ],
    ...(won
      ? {
          scoreboard: [
            {
              actor: 0,
              tag: "Player",
              damageDone: 120,
              damageTaken: 40,
              healingDone: 30,
              healingReceived: 0,
              killingBlows: 1,
            },
            {
              actor: 1,
              tag: "Opponent1",
              damageDone: 40,
              damageTaken: 120,
              healingDone: 0,
              healingReceived: 0,
              killingBlows: 0,
            },
            {
              actor: 2,
              tag: "Team1",
              damageDone: 80,
              damageTaken: 20,
              healingDone: 0,
              healingReceived: 30,
              killingBlows: 1,
            },
            {
              actor: 3,
              tag: "Opponent2",
              damageDone: 20,
              damageTaken: 80,
              healingDone: 0,
              healingReceived: 0,
              killingBlows: 0,
            },
          ],
        }
      : { scoreboard: null }),
  });
}

afterEach(() => {
  cleanup();
  input.cancel();
  engine.set(null);
  nameplateScene.set(null);
  focus.set(null);
  beforeMatch.set(false);
  customizerVisible.set(false);
  customLobbyVisible.set(false);
  clearPostMatchScoreboard();
  matchQueue.set(null);
  networkLink.set(null);
  error.set("");
  setPanel(null);
  interaction.set({ panel: null, quick: false, layout: false, hovered: null, capture: null });
});

it("renders read-only spectator HUD with both teams, camera controls, no action bars, and blocks combat commands", () => {
  const snap = spectatorSnapshot({ follow: null });
  engine.set(snap);
  nameplateScene.set(snap);
  beforeMatch.set(false);
  const linkState = createStore<LinkState>({
    phase: "open",
    attempt: 0,
    ended: null,
    status: {
      phase: "playing",
      error: null,
      seat: null,
      spectator: true,
      mapDiffers: false,
      rttMs: 25,
      inputMs: null,
      delayMs: 100,
      snapshots: 40,
      underruns: 0,
      hashMismatches: 0,
      predictedTicks: 0,
      corrections: 0,
      lastCorrectionMm: null,
      queued: null,
      queuedMin: null,
      pace: 1,
      skippedInputs: 0,
      stepsPerSecond: 30,
      sentPerSecond: 0,
      tickRate: 30,
    },
  });
  let left = 0;
  networkLink.set({
    state: linkState,
    leave: () => {
      left += 1;
      linkState.set({ phase: "idle", attempt: 0, ended: null, status: null });
    },
    remembered: () => null,
  } as unknown as ReturnType<typeof networkLink.get>);

  render(<App />);

  // Both teams' frames appear with neutral labels, cast bars, lockout, and disconnected state.
  const teamA = screen.getByRole("region", { name: "Team A" });
  const teamB = screen.getByRole("region", { name: "Team B" });
  expect(teamA.textContent).toContain("Team A 1");
  expect(teamA.textContent).toContain("Team A 2");
  expect(teamA.textContent).toContain("Rift Bolt");
  expect(teamA.textContent).toContain("Locked out");
  expect(teamB.textContent).toContain("Team B 1");
  expect(teamB.textContent).toContain("Team B 2");
  expect(teamB.textContent).toContain("Disconnected");

  // No combat action bars or target frames are rendered.
  expect(screen.queryByRole("toolbar", { name: /Action bar/i })).toBeNull();
  expect(screen.queryByRole("region", { name: "target status" })).toBeNull();

  // Round clock and spectator toolbar are visible.
  expect(screen.getByRole("timer", { name: "Round time remaining" })).toBeTruthy();
  const bar = screen.getByRole("toolbar", { name: "Spectator camera controls" });
  expect(bar.textContent).toContain("Spectating");
  expect(bar.textContent).toContain("Free camera");

  // Combat, targeting, spellbook, and layout commands are blocked while spectating.
  activateSlot("b1s1");
  interactUnit("Opponent1", "target");
  interactEntity("practice-opponent", "focus");
  dispatch("spellbook");
  dispatch("layout");
  dispatch("quickBind");
  expect(interaction.get().panel).toBeNull();
  expect(focus.get()).toBeNull();
  const drained = input.readInput();
  expect(drained[2]).toBe(0); // attack
  expect(drained[17]).toBe(-1); // selectActor
  expect(drained[19]).toBe(0); // castSpell

  // Clicking a fighter frame sends follow actor command; Free camera sends -1.
  fireEvent.click(within(teamB).getByRole("button", { name: "Follow Team B 1" }));
  expect(input.readInput()[20]).toBe(1);

  act(() => {
    engine.set(spectatorSnapshot({ follow: 1 }));
  });
  expect(bar.textContent).toContain("Following Team B 1");

  fireEvent.click(within(bar).getByRole("button", { name: "Follow next fighter" }));
  expect(input.readInput()[20]).toBe(2);

  fireEvent.click(within(bar).getByRole("button", { name: "Free camera" }));
  expect(input.readInput()[20]).toBe(-1);

  // Actor 0 nameplate is visible while spectating and clicking it follows actor 0.
  const plates = screen.getByLabelText("World health bars");
  fireEvent.click(within(plates).getByRole("button", { name: /Team A 1.*Follow fighter/ }));
  expect(input.readInput()[20]).toBe(0);

  // Game menu shows spectator mode and hides Spells, Edit layout, and Quick binding.
  act(() => setPanel("menu"));
  const menu = screen.getByRole("dialog", { name: "Game menu" });
  expect(menu.textContent).toContain("Spectating match");
  expect(within(menu).queryByRole("button", { name: "Spells" })).toBeNull();
  expect(within(menu).queryByRole("button", { name: "Edit layout" })).toBeNull();
  expect(within(menu).queryByRole("button", { name: "Quick binding" })).toBeNull();
  act(() => setPanel(null));

  // Leave button disconnects the spectator link.
  const liveBar = screen.getByRole("toolbar", { name: "Spectator camera controls" });
  fireEvent.click(within(liveBar).getByRole("button", { name: "Leave" }));
  expect(left).toBe(1);
});

it("shows neutral Team A / Team B post-match scoreboard for spectators without You highlight or public requeue", () => {
  const linkState = createStore<LinkState>({
    phase: "ended",
    attempt: 0,
    ended: "MatchOver",
    status: {
      phase: "finished",
      error: null,
      seat: null,
      spectator: true,
      mapDiffers: false,
      rttMs: 20,
      inputMs: null,
      delayMs: 100,
      snapshots: 120,
      underruns: 0,
      hashMismatches: 0,
      predictedTicks: 0,
      corrections: 0,
      lastCorrectionMm: null,
      queued: null,
      queuedMin: null,
      pace: 1,
      skippedInputs: 0,
      stepsPerSecond: 30,
      sentPerSecond: 0,
      tickRate: 30,
    },
  });
  networkLink.set({
    state: linkState,
    leave: () => linkState.set({ phase: "idle", attempt: 0, ended: null, status: null }),
    remembered: () => null,
  } as unknown as ReturnType<typeof networkLink.get>);
  matchQueue.set({
    state: createStore({ ...IDLE_QUEUE, connection: "online" }),
    chat: createStore(EMPTY_CHAT),
    showChat: () => {},
  } as unknown as ReturnType<typeof matchQueue.get>);

  engine.set(spectatorSnapshot({ won: true, roundResult: "won" }));
  beforeMatch.set(false);
  render(<App />);

  const dialog = screen.getByRole("dialog", { name: "Team A wins" });
  expect(dialog.textContent).toContain("Team A");
  expect(dialog.textContent).toContain("Team B");
  expect(dialog.textContent).not.toContain("Your team");
  expect(dialog.textContent).not.toContain("You");
  expect(within(dialog).queryByRole("button", { name: "Queue again" })).toBeNull();
  expect(within(dialog).getByRole("button", { name: "Leave" })).toBeTruthy();
});

it("keeps Space rise held continuously in spectator free-camera and clears spellbook/layout modes on entering spectating", () => {
  // Start with Spellbook + layout mode active (simulating minimized lobby -> match start).
  interaction.set({ panel: "spellbook", quick: false, layout: true, hovered: null, capture: null });
  engine.set(spectatorSnapshot({ follow: null }));
  beforeMatch.set(false);
  render(<App />);

  // Entering spectating clears incompatible combat modals/modes so camera input & Escape are never soft-locked.
  expect(interaction.get().panel).toBeNull();
  expect(interaction.get().layout).toBe(false);
  expect(interaction.get().quick).toBe(false);

  const canvas = document.createElement("canvas");
  document.body.append(canvas);
  const router = installInput(canvas, () => {});
  try {
    window.dispatchEvent(new KeyboardEvent("keydown", { code: "Space", bubbles: true }));
    expect(router.sample()[14]).toBe(1);
    // Second consecutive sample while Space is still held stays 1 for free-cam vertical flight.
    expect(router.sample()[14]).toBe(1);
    window.dispatchEvent(new KeyboardEvent("keyup", { code: "Space", bubbles: true }));
    expect(router.sample()[14]).toBe(0);
  } finally {
    router.dispose();
    canvas.remove();
  }
});
