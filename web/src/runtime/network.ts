// The browser end of a network match (ADR 0044, Phase 5): owns the
// WebSocket, reconnects with backoff and remembers the ticket so a dropped
// connection, reload or closed tab can rejoin. Rust owns the protocol: it
// reads `[generation, ticket, ...frames]` each frame, sends encoded frames
// back, and reports the match status. Nothing here interprets game data.

import {
  backoffMs,
  isExpired,
  isFinalError,
  parseStatus,
  parseTicket,
  type MatchTicket,
  type NetworkStatus,
} from "../domain/network";
import { createStore } from "../state/store";

export type LinkPhase = "idle" | "connecting" | "open" | "reconnecting" | "ended";
export type LinkState = {
  phase: LinkPhase;
  /** Reconnect attempts since the last open socket. */
  attempt: number;
  /** Why the link ended: a server error code or `expired`. */
  ended: string | null;
  /** The latest status Rust reported, if any. */
  status: NetworkStatus | null;
};

type SocketLike = {
  binaryType: string;
  readyState: number;
  onopen: (() => void) | null;
  onclose: (() => void) | null;
  onerror: (() => void) | null;
  onmessage: ((event: { data: unknown }) => void) | null;
  send(data: Uint8Array): void;
  close(): void;
};
type StorageLike = Pick<Storage, "getItem" | "setItem" | "removeItem">;
export type LinkDeps = {
  createSocket(url: string): SocketLike;
  now(): number;
  setTimer(run: () => void, ms: number): unknown;
  clearTimer(id: unknown): void;
  /** Per tab: survives a reload, so the tab rejoins by itself. */
  tab: StorageLike | null;
  /** Per browser: remembers the last match for an offered rejoin. */
  device: StorageLike | null;
};

const TAB_KEY = "arena.match";
const DEVICE_KEY = "arena.lastMatch";
/** Frames held while Rust is not reading (a hidden tab) before dropping old ones. */
const MAX_QUEUED = 512;
const OPEN = 1;

function load(storage: StorageLike | null, key: string, now: number) {
  try {
    const stored = storage?.getItem(key);
    const ticket = stored ? parseTicket(JSON.parse(stored)) : null;
    return ticket && !isExpired(ticket, now) ? ticket : null;
  } catch {
    return null;
  }
}
function store(storage: StorageLike | null, key: string, ticket: MatchTicket | null) {
  try {
    if (ticket) storage?.setItem(key, JSON.stringify(ticket));
    else storage?.removeItem(key);
  } catch {
    // Storage can be unavailable (private modes); the match still runs.
  }
}

export function createLink(deps: LinkDeps) {
  const state = createStore<LinkState>({
    phase: "idle",
    attempt: 0,
    ended: null,
    status: null,
  });
  let ticket: MatchTicket | null = null;
  let socket: SocketLike | null = null;
  let generation = 0;
  let opened = 0;
  let queue: Uint8Array[] = [];
  let timer: unknown = null;

  const update = (patch: Partial<LinkState>) => state.set({ ...state.get(), ...patch });
  const stopTimer = () => {
    if (timer !== null) deps.clearTimer(timer);
    timer = null;
  };
  const dropSocket = () => {
    if (!socket) return;
    socket.onopen = socket.onclose = socket.onerror = socket.onmessage = null;
    try {
      socket.close();
    } catch {
      // Already closed.
    }
    socket = null;
    generation = 0;
    queue = [];
  };
  const end = (reason: string | null) => {
    stopTimer();
    dropSocket();
    ticket = null;
    store(deps.tab, TAB_KEY, null);
    // The seat is gone for good: do not offer it again later.
    if (reason !== null) store(deps.device, DEVICE_KEY, null);
    update({ phase: reason === null ? "idle" : "ended", ended: reason, attempt: 0 });
  };
  const connect = () => {
    stopTimer();
    if (!ticket) return;
    if (isExpired(ticket, deps.now())) return end("expired");
    dropSocket();
    let next: SocketLike;
    try {
      next = deps.createSocket(ticket.server);
    } catch {
      return retry();
    }
    socket = next;
    next.binaryType = "arraybuffer";
    next.onopen = () => {
      opened += 1;
      generation = opened;
      queue = [];
      update({ phase: "open", attempt: 0 });
    };
    next.onmessage = (event) => {
      if (!(event.data instanceof ArrayBuffer)) return;
      queue.push(new Uint8Array(event.data));
      if (queue.length > MAX_QUEUED) queue.splice(0, queue.length - MAX_QUEUED);
    };
    next.onclose = next.onerror = () => {
      if (socket !== next) return;
      next.onopen = next.onclose = next.onerror = next.onmessage = null;
      socket = null;
      generation = 0;
      retry();
    };
    update({ phase: state.get().attempt ? "reconnecting" : "connecting" });
  };
  const retry = () => {
    if (!ticket || timer !== null) return;
    const attempt = state.get().attempt;
    update({ phase: "reconnecting", attempt: attempt + 1 });
    timer = deps.setTimer(connect, backoffMs(attempt));
  };

  return {
    state,
    /** Joins (or rejoins) a seat, remembering it for this tab and device. */
    join(next: MatchTicket) {
      end(null);
      ticket = next;
      store(deps.tab, TAB_KEY, next);
      store(deps.device, DEVICE_KEY, next);
      update({ phase: "connecting", attempt: 0, ended: null, status: null });
      connect();
    },
    /** Leaves the match on this tab; the device still offers a rejoin. */
    leave() {
      end(null);
    },
    /** Closes the socket without forgetting the match (runtime teardown). */
    dispose() {
      stopTimer();
      dropSocket();
      ticket = null;
    },
    /** This tab's match after a reload, if it has one. */
    resumable(): MatchTicket | null {
      return load(deps.tab, TAB_KEY, deps.now());
    },
    /** The last match joined on this device, for an offered rejoin. */
    remembered(): MatchTicket | null {
      return load(deps.device, DEVICE_KEY, deps.now());
    },
    /** Rust's per-frame read: empty without a match. */
    read(): unknown[] {
      if (!ticket) return [];
      const frames = queue;
      queue = [];
      return [generation, ticket.ticket, ...frames];
    },
    send(frame: Uint8Array) {
      if (socket?.readyState === OPEN) socket.send(frame);
    },
    /** Rust's status report; a final error ends the link. */
    onStatus(raw: unknown) {
      const status = parseStatus(raw);
      if (!status) return;
      update({ status });
      if (status.phase === "closed" && isFinalError(status.error)) end(status.error);
    },
  };
}
export type Link = ReturnType<typeof createLink>;

export function browserLink() {
  const safe = (get: () => Storage) => {
    try {
      return get();
    } catch {
      return null;
    }
  };
  return createLink({
    createSocket: (url) => new WebSocket(url) as unknown as SocketLike,
    now: () => Date.now(),
    setTimer: (run, ms) => window.setTimeout(run, ms),
    clearTimer: (id) => window.clearTimeout(id as number),
    tab: safe(() => window.sessionStorage),
    device: safe(() => window.localStorage),
  });
}
