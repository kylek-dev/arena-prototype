import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import type { MatchTicket } from "../../web/src/domain/network";
import { MatchStatus } from "../../web/src/features/match-status/MatchStatus";
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

it("queues, confirms the ready check by itself and joins the seat it is given", () => {
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
  fake.say({ type: "queued", mode: "2v2", waiting: 3 });
  expect(queue.state.get()).toMatchObject({ phase: "queued", waiting: 3, since: 1000 });
  fake.tick(5000);
  fake.say({ type: "queued", mode: "2v2", waiting: 2 });
  expect(queue.state.get().since).toBe(1000);
  fake.say({ type: "found", mode: "2v2" });
  expect(queue.state.get().phase).toBe("found");
  expect(fake.sent().at(-1)).toEqual({ type: "ready" });
  // Someone else did not confirm: searching again, still timed from the start.
  fake.say({ type: "requeued", mode: "2v2" });
  expect(queue.state.get()).toMatchObject({ phase: "queued", since: 1000 });
  fake.say(MATCH);
  expect(fake.joined).toEqual([
    { server: "ws://127.0.0.1:8080/play", ticket: "ab01", expires: 2_000_000_000 },
  ]);
  // Still connected afterwards, so a party stays together.
  fake.say(ALONE);
  expect(queue.state.get()).toMatchObject({ connection: "online", phase: "idle" });
  expect(fake.latest().closed).toBe(false);
});

it("forms and joins parties, and tells the player what the door refused", () => {
  const fake = fakeQueue();
  const { queue } = fake;
  queue.createParty();
  fake.open();
  expect(fake.sent().at(-1)).toEqual({ type: "create_party" });
  fake.say({ type: "party", code: "K7P2QX", members: 1, leader: true, queue: null });
  expect(queue.state.get().party).toEqual({ code: "K7P2QX", members: 1, leader: true });
  // Joining another party reuses the connection.
  expect(queue.joinParty(" m9q3rt ")).toBe(true);
  expect(fake.sent().at(-1)).toEqual({ type: "join_party", code: "M9Q3RT" });
  expect(fake.sockets).toHaveLength(1);
  fake.say({ type: "refused", code: "party_full" });
  expect(queue.state.get().notice).toBe("party_full");
  // A code that cannot exist is refused without asking the door.
  expect(queue.joinParty("hello!")).toBe(false);
  expect(queue.state.get().notice).toBe("no_such_party");
  // The leader's search is everyone's; a member leaving stops it.
  fake.say({ type: "party", code: "K7P2QX", members: 2, leader: false, queue: "3v3" });
  expect(queue.state.get()).toMatchObject({ phase: "queued", mode: "3v3" });
  fake.say({ type: "stopped", reason: "member_left" });
  expect(queue.state.get()).toMatchObject({ phase: "idle", notice: "member_left" });
  queue.leaveParty();
  expect(fake.sent().at(-1)).toEqual({ type: "leave_party" });
});

it("keeps one guest id per browser", () => {
  expect(fakeQueue().values.get("arena.guest")).toBe("07".repeat(16));
  expect(fakeQueue("ab".repeat(16)).queue.guest).toBe("ab".repeat(16));
  expect(fakeQueue("not a guest id").queue.guest).toBe("07".repeat(16));
});

it("cancels, and ends with a reason when the door goes away", () => {
  const fake = fakeQueue();
  const { queue } = fake;
  queue.queue("1v1");
  fake.open();
  fake.say({ type: "queued", mode: "1v1", waiting: 1 });
  queue.cancel();
  expect(fake.sent().at(-1)).toEqual({ type: "cancel" });
  fake.say({ type: "stopped", reason: "cancelled" });
  expect(queue.state.get().phase).toBe("idle");

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

it("the game menu forms a party and queues, and the banner offers Cancel", () => {
  const fake = withQueue();
  render(
    <>
      <GameMenu />
      <MatchStatus />
    </>,
  );
  expect(screen.getByRole("region", { name: "Play online" })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Invite friends" }));
  fake.open();
  fake.say({ type: "party", code: "K7P2QX", members: 2, leader: true, queue: null });
  expect(screen.getByText("K7P2QX")).toBeTruthy();
  expect(screen.getByRole("button", { name: "Copy invite link" })).toBeTruthy();
  // A party of two cannot enter solo queues.
  expect(screen.getByRole("button", { name: "Solo 2v2" }).hasAttribute("disabled")).toBe(true);
  fireEvent.click(screen.getByRole("button", { name: "2v2" }));
  expect(fake.sent().at(-1)).toEqual({ type: "queue", mode: "2v2" });
  fake.say({ type: "party", code: "K7P2QX", members: 2, leader: true, queue: "2v2" });
  fake.say({ type: "queued", mode: "2v2", waiting: 4 });
  expect(screen.getByRole("status").textContent).toContain(
    "Searching for a 2v2 match · 0:00 · 4 players queued",
  );
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  expect(fake.sent().at(-1)).toEqual({ type: "cancel" });
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

it("the banner explains why online play stopped and can be dismissed", () => {
  const fake = withQueue();
  render(<MatchStatus />);
  act(() => fake.queue.queue("1v1"));
  fake.drop();
  expect(screen.getByRole("status").textContent).toContain("Could not reach the matchmaker");
  fireEvent.click(screen.getByRole("button", { name: "Dismiss" }));
  expect(screen.queryByRole("status")).toBeNull();
});
