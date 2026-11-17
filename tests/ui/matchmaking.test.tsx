import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import type { MatchTicket } from "../../web/src/domain/network";
import { MatchReady } from "../../web/src/features/queue/MatchReady";
import { QueueBar } from "../../web/src/features/queue/QueueBar";
import { GameMenu } from "../../web/src/features/menu/GameMenu";
import { createQueue, installPartyLinks, type Queue } from "../../web/src/runtime/matchmaking";
import { matchQueue } from "../../web/src/state/network";

type Fake = {
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

const MATCH = {
  type: "match",
  mode: "2v2",
  match_id: "00ff00ff00ff00ff",
  seat: 2,
  server: "ws://127.0.0.1:8080/play",
  ticket: "ab01",
  expires: 2_000_000_000,
};
const ALONE = { type: "party", code: null, members: 1, leader: true, queue: null };

/** A real queue over fake sockets, clock and storage. */
function fakeQueue(stored: string | null = null) {
  const sockets: Fake[] = [];
  const values = new Map<string, string>();
  if (stored) values.set("arena.guest", stored);
  const joined: MatchTicket[] = [];
  let now = 1000;
  const queue = createQueue({
    url: "ws://door.test/api/queue",
    createSocket: () => {
      const socket: Fake = {
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
    now: () => now,
    storage: {
      getItem: (key) => values.get(key) ?? null,
      setItem: (key, value) => void values.set(key, value),
    },
    randomBytes: (count) => new Uint8Array(count).fill(7),
    onMatch: (ticket) => joined.push(ticket),
  });
  const latest = () => sockets[sockets.length - 1];
  return {
    queue,
    sockets,
    joined,
    values,
    latest,
    sent: () => latest().sent.map((text) => JSON.parse(text)),
    tick: (ms: number) => {
      now += ms;
    },
    open: () =>
      act(() => {
        latest().readyState = 1;
        latest().onopen?.();
      }),
    say: (message: object) => act(() => latest().onmessage?.({ data: JSON.stringify(message) })),
    drop: () => act(() => latest().onclose?.()),
  };
}

afterEach(() => {
  cleanup();
  matchQueue.set(null);
});

const queued = (mode: string, waiting: number, waitedMs = 0, expectedMs: number | null = null) => ({
  type: "queued",
  mode,
  waiting,
  waited_ms: waitedMs,
  expected_ms: expectedMs,
});
const check = (accepted: number, total: number, you: boolean, remainingMs = 15_000) => ({
  type: "ready_check",
  mode: "2v2",
  accepted,
  total,
  you,
  remaining_ms: remainingMs,
});

it("queues, offers the match to accept and joins the seat it is given", () => {
  const fake = fakeQueue();
  const { queue } = fake;
  queue.queue("2v2");
  expect(queue.state.get()).toMatchObject({ connection: "connecting", mode: "2v2" });
  fake.open();
  // Hello first, then the request made while connecting.
  expect(fake.sent()).toEqual([
    { type: "hello", guest: "07".repeat(16) },
    { type: "queue", mode: "2v2" },
  ]);
  fake.say({ ...ALONE, queue: "2v2" });
  // The door's measure of the wait is used, so a party agrees on it.
  fake.say(queued("2v2", 3, 4000, 90_000));
  expect(queue.state.get()).toMatchObject({
    phase: "queued",
    waiting: 3,
    since: -3000,
    expectedMs: 90_000,
  });
  // A match is offered: nothing is accepted until the player does.
  fake.say(check(0, 4, false));
  expect(queue.state.get()).toMatchObject({
    phase: "ready",
    readyCheck: { accepted: 0, total: 4, you: false, deadline: 16_000 },
  });
  expect(fake.sent().some((m) => m.type === "accept")).toBe(false);
  queue.accept();
  expect(fake.sent().at(-1)).toEqual({ type: "accept" });
  expect(queue.state.get().readyCheck?.you).toBe(true);
  queue.accept();
  expect(fake.sent().filter((m) => m.type === "accept")).toHaveLength(1);
  fake.say(check(3, 4, true, 9000));
  expect(queue.state.get().readyCheck).toMatchObject({ accepted: 3, deadline: 10_000 });
  // Someone else did not accept: back in the queue, told why.
  fake.say({ type: "requeued", mode: "2v2" });
  expect(queue.state.get()).toMatchObject({
    phase: "queued",
    readyCheck: null,
    notice: "requeued",
  });
  fake.say(check(0, 4, false));
  queue.accept();
  fake.say(MATCH);
  expect(fake.joined).toEqual([
    { server: "ws://127.0.0.1:8080/play", ticket: "ab01", expires: 2_000_000_000 },
  ]);
  expect(queue.state.get()).toMatchObject({ phase: "idle", readyCheck: null });
  expect(fake.latest().closed).toBe(false);
});

it("declining leaves the queue", () => {
  const fake = fakeQueue();
  const { queue } = fake;
  queue.queue("1v1");
  fake.open();
  fake.say({
    type: "ready_check",
    mode: "1v1",
    accepted: 0,
    total: 2,
    you: false,
    remaining_ms: 15_000,
  });
  queue.decline();
  expect(fake.sent().at(-1)).toEqual({ type: "decline" });
  fake.say({ type: "stopped", reason: "declined" });
  expect(queue.state.get()).toMatchObject({ phase: "idle", notice: "declined" });
});

it("forms and joins parties, and tells the player what the door refused", () => {
  const fake = fakeQueue();
  const { queue } = fake;
  queue.createParty();
  fake.open();
  expect(fake.sent().at(-1)).toEqual({ type: "create_party" });
  fake.say({ type: "party", code: "K7P2QX", members: 1, leader: true, queue: null });
  expect(queue.state.get().party).toEqual({ code: "K7P2QX", members: 1, leader: true });
  expect(queue.joinParty(" m9q3rt ")).toBe(true);
  expect(fake.sent().at(-1)).toEqual({ type: "join_party", code: "M9Q3RT" });
  expect(fake.sockets).toHaveLength(1);
  fake.say({ type: "refused", code: "party_full" });
  expect(queue.state.get().notice).toBe("party_full");
  expect(queue.joinParty("hello!")).toBe(false);
  expect(queue.state.get().notice).toBe("no_such_party");
  fake.say({ type: "party", code: "K7P2QX", members: 2, leader: false, queue: "3v3" });
  expect(queue.state.get()).toMatchObject({ phase: "queued", mode: "3v3" });
  fake.say({ type: "stopped", reason: "member_not_accepted" });
  expect(queue.state.get()).toMatchObject({ phase: "idle", notice: "member_not_accepted" });
  queue.leaveParty();
  expect(fake.sent().at(-1)).toEqual({ type: "leave_party" });
});

it("keeps one guest id per browser", () => {
  expect(fakeQueue().values.get("arena.guest")).toBe("07".repeat(16));
  expect(fakeQueue("ab".repeat(16)).queue.guest).toBe("ab".repeat(16));
  expect(fakeQueue("not a guest id").queue.guest).toBe("07".repeat(16));
});

it("ends with a reason when the door goes away", () => {
  const fake = fakeQueue();
  const { queue } = fake;
  queue.queue("1v1");
  fake.open();
  fake.say({ type: "error", code: "replaced" });
  expect(queue.state.get()).toMatchObject({ connection: "ended", ended: "replaced" });
  queue.dismiss();
  expect(queue.state.get().connection).toBe("offline");
  queue.queue("1v1");
  fake.drop();
  expect(queue.state.get()).toMatchObject({ connection: "ended", ended: "unreachable" });
  queue.queue("1v1");
  fake.open();
  fake.drop();
  expect(queue.state.get()).toMatchObject({ connection: "ended", ended: "lost" });
});

it("joins the party in an invite link and removes it from the address bar", () => {
  const fake = fakeQueue();
  window.history.replaceState(null, "", "/#party=k7p2qx");
  const stop = installPartyLinks(fake.queue);
  expect(window.location.hash).toBe("");
  fake.open();
  expect(fake.sent().at(-1)).toEqual({ type: "join_party", code: "K7P2QX" });
  stop();
});

function withQueue() {
  const fake = fakeQueue();
  matchQueue.set(fake.queue as Queue);
  return fake;
}

it("the queue bar shows the queue, time waited, expected wait and players, with Cancel", () => {
  const fake = withQueue();
  render(
    <>
      <GameMenu />
      <QueueBar />
    </>,
  );
  expect(screen.getByRole("region", { name: "Play online" })).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Solo 2v2" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "3v3" }));
  fake.open();
  fake.say({ ...ALONE, queue: "3v3" });
  fake.say(queued("3v3", 4, 83_000, 150_000));
  const bar = screen.getByRole("status", { name: "Queue" });
  expect(bar.textContent).toContain("In queue · 3v3");
  expect(bar.textContent).toContain("1:23");
  expect(bar.textContent).toContain("Expected wait about 3 min · 4 players in queue");
  fireEvent.click(within(bar).getByRole("button", { name: "Cancel" }));
  expect(fake.sent().at(-1)).toEqual({ type: "cancel" });
  fake.say({ type: "stopped", reason: "cancelled" });
  fake.say(ALONE);
  expect(screen.queryByRole("status", { name: "Queue" })).toBeNull();
});

it("the match-ready window counts down, shows acceptances and takes Accept or Decline", () => {
  const fake = withQueue();
  render(
    <>
      <QueueBar />
      <MatchReady />
    </>,
  );
  act(() => fake.queue.queue("2v2"));
  fake.open();
  fake.say(check(1, 4, false, 12_000));
  const window = screen.getByRole("alertdialog", { name: "Your 2v2 match is ready" });
  expect(within(window).getByRole("timer").textContent).toContain("12");
  expect(window.textContent).toContain("1 of 4 accepted");
  expect(window.querySelectorAll(".match-ready-player.accepted")).toHaveLength(1);
  expect(document.activeElement?.textContent).toBe("Accept");
  fireEvent.click(within(window).getByRole("button", { name: "Accept" }));
  expect(fake.sent().at(-1)).toEqual({ type: "accept" });
  expect(window.textContent).toContain("Accepted · waiting for");
  expect(within(window).queryByRole("button", { name: "Decline" })).toBeNull();
  // Someone did not accept: the window closes and the bar explains.
  fake.say({ type: "requeued", mode: "2v2" });
  fake.say(queued("2v2", 3, 20_000));
  expect(screen.queryByRole("alertdialog")).toBeNull();
  expect(screen.getByRole("status", { name: "Queue" }).textContent).toContain(
    "back at the front of the queue",
  );
  // Declining a second offer leaves the queue, with the reason shown.
  fake.say(check(0, 4, false));
  fireEvent.click(screen.getByRole("button", { name: "Decline" }));
  expect(fake.sent().at(-1)).toEqual({ type: "decline" });
  fake.say({ type: "stopped", reason: "declined" });
  expect(screen.queryByRole("alertdialog")).toBeNull();
  expect(screen.getByRole("status").textContent).toContain("You declined the match");
});

it("a party member waits for the leader to choose the queue", () => {
  const fake = withQueue();
  render(<GameMenu />);
  act(() => fake.queue.joinParty("K7P2QX"));
  fake.open();
  fake.say({ type: "party", code: "K7P2QX", members: 2, leader: false, queue: null });
  expect(screen.getByText("The party leader chooses the queue.")).toBeTruthy();
  expect(screen.queryByRole("button", { name: "2v2" })).toBeNull();
  fake.say({ type: "refused", code: "no_such_party" });
  expect(screen.getByText(/No party has that code/)).toBeTruthy();
});

it("the bar explains why online play stopped and can be dismissed", () => {
  const fake = withQueue();
  render(<QueueBar />);
  act(() => fake.queue.queue("1v1"));
  fake.drop();
  expect(screen.getByRole("status").textContent).toContain("Could not reach the matchmaker");
  fireEvent.click(screen.getByRole("button", { name: "OK" }));
  expect(screen.queryByRole("status")).toBeNull();
});
