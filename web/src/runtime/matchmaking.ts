// The browser end of the solo queues (ADR 0046): one WebSocket to the front
// door while searching. It confirms ready checks by itself and hands the
// seat ticket it receives to the match link, which joins as it does for a
// rejoin, so the ticket is never shown or put in a URL.

import {
  IDLE_QUEUE,
  LEAVE_MESSAGE,
  READY_MESSAGE,
  guestId,
  isGuestId,
  joinMessage,
  parseQueueMessage,
  queueUrl,
  type QueueMode,
  type QueueState,
} from "../domain/matchmaking";
import type { MatchTicket } from "../domain/network";
import { createStore } from "../state/store";

type TextSocket = {
  readyState: number;
  onopen: (() => void) | null;
  onclose: (() => void) | null;
  onerror: (() => void) | null;
  onmessage: ((event: { data: unknown }) => void) | null;
  send(data: string): void;
  close(): void;
};
type StorageLike = Pick<Storage, "getItem" | "setItem">;
export type QueueDeps = {
  url: string;
  createSocket(url: string): TextSocket;
  now(): number;
  /** Keeps the guest id; `null` when storage is unavailable. */
  storage: StorageLike | null;
  randomBytes(count: number): Uint8Array;
  /** Joins the seat the queue found. */
  onMatch(ticket: MatchTicket): void;
};

const GUEST_KEY = "arena.guest";
const OPEN = 1;

/** This browser's guest id, created once and kept. Without storage, a new
 * one per page: queueing still works, but two tabs count as two guests. */
function loadGuest(deps: QueueDeps) {
  try {
    const stored = deps.storage?.getItem(GUEST_KEY);
    if (isGuestId(stored)) return stored;
  } catch {
    // Storage can be unavailable (private modes).
  }
  const fresh = guestId(deps.randomBytes(16));
  try {
    deps.storage?.setItem(GUEST_KEY, fresh);
  } catch {
    // Keep it for this page only.
  }
  return fresh;
}

export function createQueue(deps: QueueDeps) {
  const state = createStore<QueueState>(IDLE_QUEUE);
  const guest = loadGuest(deps);
  let socket: TextSocket | null = null;
  let opened = false;
  const update = (patch: Partial<QueueState>) => state.set({ ...state.get(), ...patch });
  const drop = () => {
    if (!socket) return;
    socket.onopen = socket.onclose = socket.onerror = socket.onmessage = null;
    socket.close();
    socket = null;
  };
  const end = (reason: string | null) => {
    drop();
    state.set(reason ? { ...state.get(), phase: "ended", ended: reason } : IDLE_QUEUE);
  };
  const receive = (data: unknown) => {
    const message = parseQueueMessage(data);
    if (!message) return;
    switch (message.type) {
      case "queued":
        update({
          phase: "queued",
          mode: message.mode,
          waiting: message.waiting,
          since: state.get().since ?? deps.now(),
        });
        break;
      case "found":
        update({ phase: "found" });
        if (socket?.readyState === OPEN) socket.send(READY_MESSAGE);
        break;
      case "requeued":
        update({ phase: "queued" });
        break;
      case "match":
        end(null);
        deps.onMatch(message.ticket);
        break;
      case "error":
        end(message.code);
        break;
    }
  };

  return {
    state,
    guest,
    /** The clock search times are measured on. */
    now: deps.now,
    /** Starts searching for `mode`, or switches an ongoing search to it. */
    join(mode: QueueMode) {
      const current = state.get();
      if (socket && current.phase !== "ended") {
        if (current.mode === mode) return;
        update({ mode, since: deps.now(), waiting: null });
        if (socket.readyState === OPEN) socket.send(joinMessage(mode, guest));
        return;
      }
      drop();
      state.set({ ...IDLE_QUEUE, phase: "connecting", mode, since: deps.now() });
      opened = false;
      const next = deps.createSocket(deps.url);
      socket = next;
      next.onopen = () => {
        opened = true;
        next.send(joinMessage(state.get().mode ?? mode, guest));
      };
      next.onmessage = (event) => receive(event.data);
      next.onerror = () => {};
      next.onclose = () => {
        if (socket !== next) return;
        socket = null;
        end(opened ? "lost" : "unreachable");
      };
    },
    /** Stops searching. */
    cancel() {
      if (socket?.readyState === OPEN) socket.send(LEAVE_MESSAGE);
      end(null);
    },
    /** Clears an ended search's message. */
    dismiss() {
      if (state.get().phase === "ended") state.set(IDLE_QUEUE);
    },
    dispose() {
      drop();
    },
  };
}
export type Queue = ReturnType<typeof createQueue>;

export function browserQueue(onMatch: (ticket: MatchTicket) => void) {
  let storage: StorageLike | null = null;
  try {
    storage = window.localStorage;
  } catch {
    storage = null;
  }
  return createQueue({
    url: queueUrl(window.location),
    createSocket: (url) => new WebSocket(url) as unknown as TextSocket,
    now: () => Date.now(),
    storage,
    randomBytes: (count) => crypto.getRandomValues(new Uint8Array(count)),
    onMatch,
  });
}
