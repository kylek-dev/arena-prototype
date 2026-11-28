import { act } from "@testing-library/react";
import type { MatchTicket } from "../../web/src/domain/network";
import { createQueue } from "../../web/src/runtime/matchmaking";

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

/** A real queue over fake sockets, clock and storage, for social tests. */
export function fakeQueue(clock: () => number = () => 1000) {
  const sockets: Fake[] = [];
  /** Tickets handed to `onMatch` (matches and hub worlds). */
  const tickets: MatchTicket[] = [];
  let worldsLeft = 0;
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
    now: clock,
    storage: null,
    randomBytes: (count) => new Uint8Array(count).fill(7),
    onMatch: (ticket) => tickets.push(ticket),
    onWorldLeft: () => {
      worldsLeft += 1;
    },
  });
  const latest = () => sockets[sockets.length - 1];
  return {
    queue,
    sockets,
    tickets,
    worldsLeft: () => worldsLeft,
    latest,
    sent: () => (latest()?.sent ?? []).map((text) => JSON.parse(text)),
    open: () =>
      act(() => {
        latest().readyState = 1;
        latest().onopen?.();
      }),
    say: (message: object) => act(() => latest().onmessage?.({ data: JSON.stringify(message) })),
  };
}
