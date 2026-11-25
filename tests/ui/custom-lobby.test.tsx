import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import type { MatchTicket } from "../../web/src/domain/network";
import { CustomLobby } from "../../web/src/features/custom-lobby/CustomLobby";
import { GameMenu } from "../../web/src/features/menu/GameMenu";
import { QueueBar } from "../../web/src/features/queue/QueueBar";
import { Scoreboard } from "../../web/src/features/scoreboard/Scoreboard";
import { createQueue, installPartyLinks, type Queue } from "../../web/src/runtime/matchmaking";
import { matchQueue, networkLink } from "../../web/src/state/network";
import { clearPostMatchScoreboard } from "../../web/src/state/scoreboard";
import { customLobbyVisible, engine, error, setPanel } from "../../web/src/state/session";
import { createStore } from "../../web/src/state/store";
import { adaptSnapshot } from "../../web/src/bridge/snapshot";
import type { LinkState } from "../../web/src/domain/network";
import { teamSnapshot } from "./fixtures";

type FakeSocket = {
  readyState: number;
  sent: string[];
  closed: boolean;
  onopen: (() => void) | null;
  onclose: (() => void) | null;
  onerror: (() => void) | null;
  onmessage: ((event: { data: unknown }) => void) | null;
  send(data: string): void;
  close(): void;
};

function fakeQueue() {
  const sockets: FakeSocket[] = [];
  const joined: MatchTicket[] = [];
  const queue = createQueue({
    url: "ws://door.test/api/queue",
    createSocket: () => {
      const socket: FakeSocket = {
        readyState: 0,
        sent: [],
        closed: false,
        onopen: null,
        onclose: null,
        onerror: null,
        onmessage: null,
        send(data) {
          this.sent.push(data);
        },
        close() {
          this.closed = true;
        },
      };
      sockets.push(socket);
      return socket;
    },
    now: () => 1000,
    storage: null,
    randomBytes: (count) => new Uint8Array(count).fill(3),
    onMatch: (ticket) => {
      customLobbyVisible.set(false);
      joined.push(ticket);
    },
  });
  const latest = () => sockets[sockets.length - 1];
  return {
    queue: queue as Queue,
    joined,
    sent: () => latest().sent.map((text) => JSON.parse(text)),
    open: () =>
      act(() => {
        latest().readyState = 1;
        latest().onopen?.();
      }),
    say: (message: object) => act(() => latest().onmessage?.({ data: JSON.stringify(message) })),
  };
}

function finishedNet1v1() {
  const base = teamSnapshot(1);
  return adaptSnapshot({
    ...base,
    maxHealth: 100,
    tick: 300,
    maxRoundTicks: 9000,
    tickRate: 30,
    won: true,
    roundResult: "won",
    scoreboard: [
      {
        actor: 0,
        tag: "Player",
        damageDone: 100,
        damageTaken: 40,
        healingDone: 0,
        healingReceived: 0,
        killingBlows: 1,
      },
      {
        actor: 1,
        tag: "Opponent1",
        damageDone: 40,
        damageTaken: 100,
        healingDone: 0,
        healingReceived: 0,
        killingBlows: 0,
      },
    ],
    roster: base.roster?.map((unit) => ({
      actor: unit.actor,
      tag: unit.tag,
      health: unit.actor === 0 ? 60 : 0,
      connected: true,
    })),
  });
}

afterEach(() => {
  cleanup();
  matchQueue.set(null);
  networkLink.set(null);
  customLobbyVisible.set(false);
  clearPostMatchScoreboard();
  engine.set(null);
  error.set("");
  setPanel(null);
});

it("hosts a custom lobby from the game menu, controls teams/settings/host, and starts when full", () => {
  const fake = fakeQueue();
  matchQueue.set(fake.queue);
  render(
    <>
      <GameMenu />
      <CustomLobby />
      <QueueBar />
    </>,
  );

  fireEvent.click(screen.getByRole("button", { name: "Host custom match" }));
  fake.open();
  expect(fake.sent().at(-1)).toEqual({ type: "create_lobby", mode: "1v1" });

  fake.say({
    type: "lobby",
    code: "K7P2QX",
    mode: "1v1",
    settings: { version: 1, map: "colosseum", round_seconds: 1200 },
    spectator_slots: 4,
    you: 1,
    host: true,
    can_start: false,
    members: [{ id: 1, slot: "team_a", host: true, you: true }],
  });

  const dialog = screen.getByRole("dialog", { name: /Custom match · 1v1/ });
  expect(dialog.textContent).toContain("K7P2QX");
  expect(within(dialog).getByRole("button", { name: "Start match" })).toHaveProperty(
    "disabled",
    true,
  );

  // Host changes round time limit and spectator toggle.
  fireEvent.change(within(dialog).getByLabelText("Round time limit"), {
    target: { value: "300" },
  });
  expect(fake.sent().at(-1)).toEqual({
    type: "set_lobby_settings",
    settings: { version: 1, map: "colosseum", round_seconds: 300, allow_spectators: true },
  });
  fireEvent.change(within(dialog).getByLabelText("Allow spectators"), {
    target: { value: "off" },
  });
  expect(fake.sent().at(-1)).toEqual({
    type: "set_lobby_settings",
    settings: { version: 1, map: "colosseum", round_seconds: 1200, allow_spectators: false },
  });

  // Second player arrives -> lobby is now full and startable.
  fake.say({
    type: "lobby",
    code: "K7P2QX",
    mode: "1v1",
    settings: { version: 1, map: "colosseum", round_seconds: 300 },
    spectator_slots: 4,
    you: 1,
    host: true,
    can_start: true,
    members: [
      { id: 1, slot: "team_a", host: true, you: true },
      { id: 2, slot: "team_b", host: false, you: false },
    ],
  });

  // Every slot action shares one bordered, clickable style (never ghost text).
  for (const name of [
    "Move Player 2 to Team A",
    "Move Player 2 to Spectators",
    "Make Player 2 host",
    "Remove Player 2",
  ]) {
    const action = within(dialog).getByRole("button", { name });
    expect(action.classList.contains("action-button"), name).toBe(true);
    expect(action.classList.contains("rt-variant-surface"), name).toBe(true);
    expect(action.classList.contains("rt-variant-ghost"), name).toBe(false);
  }

  // Host moves player 2, passes host, and removes player 2.
  fireEvent.click(within(dialog).getByRole("button", { name: "Move Player 2 to Team A" }));
  expect(fake.sent().at(-1)).toEqual({ type: "move_lobby_member", member: 2, slot: "team_a" });
  fireEvent.click(within(dialog).getByRole("button", { name: "Make Player 2 host" }));
  expect(fake.sent().at(-1)).toEqual({ type: "transfer_lobby_host", member: 2 });
  fireEvent.click(within(dialog).getByRole("button", { name: "Remove Player 2" }));
  expect(fake.sent().at(-1)).toEqual({ type: "kick_lobby_member", member: 2 });

  // With handles from the door, members are named by them.
  fake.say({
    type: "lobby",
    code: "K7P2QX",
    mode: "1v1",
    settings: { version: 1, map: "colosseum", round_seconds: 300 },
    spectator_slots: 4,
    you: 1,
    host: true,
    can_start: true,
    members: [
      { id: 1, slot: "team_a", host: true, you: true, handle: "Rook#0042" },
      { id: 2, slot: "team_b", host: false, you: false, handle: "Guest#0007" },
    ],
  });
  expect(within(dialog).getByText("Rook#0042")).toBeTruthy();
  fireEvent.click(within(dialog).getByRole("button", { name: "Remove Guest#0007" }));
  expect(fake.sent().at(-1)).toEqual({ type: "kick_lobby_member", member: 2 });

  // Minimize shows the top bar pill; clicking Open lobby brings the dialog back.
  fireEvent.click(within(dialog).getByRole("button", { name: "Minimize" }));
  expect(screen.queryByRole("dialog")).toBeNull();
  const bar = screen.getByRole("status", { name: "Custom lobby" });
  expect(bar.textContent).toContain("K7P2QX");
  fireEvent.click(within(bar).getByRole("button", { name: "Open lobby" }));

  // Starting sends start_lobby and hides the modal when the seat ticket arrives.
  const reopened = screen.getByRole("dialog", { name: /Custom match · 1v1/ });
  const startButton = within(reopened).getByRole("button", { name: "Start match" });
  expect(startButton).toHaveProperty("disabled", false);
  fireEvent.click(startButton);
  expect(fake.sent().at(-1)).toEqual({ type: "start_lobby" });

  fake.say({
    type: "match",
    mode: "1v1",
    match_id: "0123456789abcdef",
    seat: 0,
    server: "ws://127.0.0.1:8080/play",
    ticket: "abcd",
    expires: 2_000_000_000,
  });
  expect(fake.joined).toHaveLength(1);
  expect(screen.queryByRole("dialog")).toBeNull();
});

it("joins a custom lobby from a #custom= invite fragment and returns to lobby from the scoreboard", () => {
  const fake = fakeQueue();
  matchQueue.set(fake.queue);
  window.history.replaceState(null, "", "/#custom=m9q3rt");
  const stop = installPartyLinks(fake.queue);
  expect(window.location.hash).toBe("");
  fake.open();
  expect(fake.sent().at(-1)).toEqual({ type: "join_lobby", code: "M9Q3RT" });
  stop();

  const linkState = createStore<LinkState>({
    phase: "ended",
    attempt: 0,
    ended: "MatchOver",
    status: null,
  });
  let leftMatch = 0;
  networkLink.set({
    state: linkState,
    leave: () => {
      leftMatch += 1;
      linkState.set({ phase: "idle", attempt: 0, ended: null, status: null });
    },
  } as unknown as ReturnType<typeof networkLink.get>);

  // Lobby stays open while the match finishes.
  fake.say({
    type: "lobby",
    code: "M9Q3RT",
    mode: "1v1",
    settings: { version: 1, map: "colosseum", round_seconds: 600 },
    spectator_slots: 4,
    you: 1,
    host: true,
    can_start: true,
    members: [
      { id: 1, slot: "team_a", host: true, you: true },
      { id: 2, slot: "team_b", host: false, you: false },
    ],
  });
  customLobbyVisible.set(false);
  engine.set(finishedNet1v1());

  render(
    <>
      <Scoreboard />
      <CustomLobby />
      <QueueBar />
    </>,
  );

  // Scoreboard suppresses the minimized custom lobby QueueBar pill and offers Return to lobby.
  expect(screen.queryByRole("status", { name: "Custom lobby" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Queue again" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Return to lobby" }));
  expect(leftMatch).toBe(1);
  expect(screen.getByRole("dialog", { name: /Custom match · 1v1/ })).toBeTruthy();
});

it("handles non-host controls, spectator notices, kicks, and suppresses auto-open during active matches", () => {
  const fake = fakeQueue();
  matchQueue.set(fake.queue);
  const linkState = createStore<LinkState>({
    phase: "open",
    attempt: 0,
    ended: null,
    status: null,
  });
  networkLink.set({
    state: linkState,
    leave: () => linkState.set({ phase: "idle", attempt: 0, ended: null, status: null }),
  } as unknown as ReturnType<typeof networkLink.get>);

  render(
    <>
      <CustomLobby />
      <QueueBar />
    </>,
  );

  fake.queue.joinLobby("K7P2QX");
  fake.open();
  // Receiving a lobby during an active match must NOT auto-open over gameplay.
  fake.say({
    type: "lobby",
    code: "K7P2QX",
    mode: "2v2",
    settings: { version: 1, map: "colosseum", round_seconds: 600 },
    spectator_slots: 4,
    you: 2,
    host: false,
    can_start: false,
    members: [
      { id: 1, slot: "team_a", host: true, you: false },
      { id: 2, slot: "team_b", host: false, you: true },
    ],
  });
  expect(screen.queryByRole("dialog")).toBeNull();

  // Once the match ends/closes and the player opens the lobby, non-host sees read-only settings and self slot switch.
  act(() => {
    linkState.set({ phase: "idle", attempt: 0, ended: null, status: null });
    customLobbyVisible.set(true);
  });
  const dialog = screen.getByRole("dialog", { name: /Custom match · 2v2/ });
  expect(within(dialog).getByLabelText("Match size")).toHaveProperty("disabled", true);
  expect(within(dialog).getByLabelText("Round time limit")).toHaveProperty("disabled", true);
  expect(within(dialog).queryByRole("button", { name: "Start match" })).toBeNull();
  expect(within(dialog).queryByRole("button", { name: /Remove Player/ })).toBeNull();

  fireEvent.click(within(dialog).getByRole("button", { name: "Join spectators" }));
  expect(fake.sent().at(-1)).toEqual({ type: "set_lobby_slot", slot: "spectator" });

  // Spectator receives a live signed spectator match ticket when the host starts.
  fake.say({
    type: "match",
    mode: "2v2",
    match_id: "0123456789abcdef",
    seat: 0,
    spectator: true,
    server: "ws://127.0.0.1:8080/play",
    ticket: "beef",
    expires: 2_000_000_000,
  });
  expect(fake.joined.at(-1)).toMatchObject({ ticket: "beef", spectator: true });
  expect(screen.queryByRole("dialog")).toBeNull();

  // Kicked from the lobby shows the QueueBar warning notice.
  fake.say({ type: "lobby_left", reason: "kicked" });
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(screen.getByRole("status").textContent).toContain("removed from the custom lobby");
});
