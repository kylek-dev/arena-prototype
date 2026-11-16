import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import type { MatchTicket } from "../../web/src/domain/network";
import { MatchStatus } from "../../web/src/features/match-status/MatchStatus";
import { GameMenu } from "../../web/src/features/menu/GameMenu";
import { createQueue, type Queue } from "../../web/src/runtime/matchmaking";
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
  mode: "solo-2v2",
  match_id: "00ff00ff00ff00ff",
  seat: 2,
  server: "ws://127.0.0.1:8080/play",
  ticket: "ab01",
  expires: 2_000_000_000,
};

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
  queue.join("solo-2v2");
  expect(queue.state.get()).toMatchObject({ phase: "connecting", mode: "solo-2v2" });
  fake.open();
  expect(JSON.parse(fake.latest().sent[0])).toEqual({
    type: "join",
    mode: "solo-2v2",
    guest: "07".repeat(16),
  });
  fake.say({ type: "queued", mode: "solo-2v2", waiting: 3 });
  expect(queue.state.get()).toMatchObject({ phase: "queued", waiting: 3, since: 1000 });
  fake.tick(5000);
  fake.say({ type: "queued", mode: "solo-2v2", waiting: 2 });
  expect(queue.state.get().since).toBe(1000);
  fake.say({ type: "found", mode: "solo-2v2" });
  expect(queue.state.get().phase).toBe("found");
  expect(JSON.parse(fake.latest().sent[1])).toEqual({ type: "ready" });
  // Someone else did not confirm: searching again, still timed from the start.
  fake.say({ type: "requeued", mode: "solo-2v2" });
  expect(queue.state.get()).toMatchObject({ phase: "queued", since: 1000 });
  fake.say(MATCH);
  expect(fake.joined).toEqual([
    { server: "ws://127.0.0.1:8080/play", ticket: "ab01", expires: 2_000_000_000 },
  ]);
  expect(queue.state.get().phase).toBe("idle");
  expect(fake.latest().closed).toBe(true);
});

it("keeps one guest id per browser", () => {
  const first = fakeQueue();
  expect(first.values.get("arena.guest")).toBe("07".repeat(16));
  const again = fakeQueue("ab".repeat(16));
  expect(again.queue.guest).toBe("ab".repeat(16));
  const corrupt = fakeQueue("not a guest id");
  expect(corrupt.queue.guest).toBe("07".repeat(16));
});

it("cancels, switches modes, and ends with a reason", () => {
  const fake = fakeQueue();
  const { queue } = fake;
  queue.join("1v1");
  fake.open();
  fake.say({ type: "queued", mode: "1v1", waiting: 1 });
  // Switching modes reuses the connection.
  queue.join("solo-3v3");
  expect(fake.sockets).toHaveLength(1);
  expect(JSON.parse(fake.latest().sent[1])).toMatchObject({ type: "join", mode: "solo-3v3" });
  queue.cancel();
  expect(JSON.parse(fake.latest().sent[2])).toEqual({ type: "leave" });
  expect(queue.state.get().phase).toBe("idle");

  queue.join("1v1");
  fake.open();
  fake.say({ type: "error", code: "replaced" });
  expect(queue.state.get()).toMatchObject({ phase: "ended", ended: "replaced" });
  queue.dismiss();
  expect(queue.state.get().phase).toBe("idle");

  // A door that never answers, and one that goes away mid-search.
  queue.join("1v1");
  fake.drop();
  expect(queue.state.get()).toMatchObject({ phase: "ended", ended: "unreachable" });
  queue.join("1v1");
  fake.open();
  fake.drop();
  expect(queue.state.get()).toMatchObject({ phase: "ended", ended: "lost" });
  // Messages that are not understood change nothing.
  queue.join("1v1");
  fake.open();
  act(() => fake.latest().onmessage?.({ data: "{}" }));
  expect(queue.state.get().phase).toBe("connecting");
});

function withQueue() {
  const fake = fakeQueue();
  matchQueue.set(fake.queue as Queue);
  return fake;
}

it("the game menu queues for a mode and the banner offers Cancel", () => {
  const fake = withQueue();
  render(
    <>
      <GameMenu />
      <MatchStatus />
    </>,
  );
  expect(screen.getByRole("group", { name: "Play online" })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Solo 3v3" }));
  expect(fake.queue.state.get()).toMatchObject({ phase: "connecting", mode: "solo-3v3" });
  fake.open();
  fake.say({ type: "queued", mode: "solo-3v3", waiting: 4 });
  expect(screen.getByRole("status").textContent).toContain(
    "Searching for a Solo 3v3 match · 0:00 · 4 players queued",
  );
  expect(screen.getByRole("button", { name: "Cancel search" })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  expect(fake.queue.state.get().phase).toBe("idle");
  expect(screen.queryByRole("status")).toBeNull();
  expect(screen.getByRole("button", { name: "1v1" })).toBeTruthy();
});

it("the banner explains why a search ended and can be dismissed", () => {
  const fake = withQueue();
  render(<MatchStatus />);
  act(() => fake.queue.join("1v1"));
  fake.drop();
  expect(screen.getByRole("status").textContent).toContain("Could not reach the matchmaker");
  fireEvent.click(screen.getByRole("button", { name: "Dismiss" }));
  expect(screen.queryByRole("status")).toBeNull();
});
