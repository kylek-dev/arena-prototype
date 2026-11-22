// The browser end of a network match (ADR 0044, Phase 5): owns the
// WebSocket, greets the server, reconnects with backoff and remembers the
// ticket so a dropped connection, reload or closed tab can rejoin.
//
// Browsers pause a background tab's animation frames, and with them the
// whole engine. So this link does not depend on the engine: it sends the
// `Hello` itself when a socket opens (keeping the seat while the tab is in
// the background or still loading) and reads the server's two-byte `Error`
// frames itself (stopping on a final error even while paused). Everything
// else is the engine's: it reads `[generation, ...frames]` each frame, sends
// inputs and pings back, and reports the match status.

import type { Appearance } from "../domain/appearance";
import {
  backoffMs,
  encodeHello,
  endsLink,
  isAppearances,
  isExpired,
  isWelcome,
  parseJoinFragment,
  parseStatus,
  parseTicket,
  serverError,
  type LinkState,
  type MatchTicket,
} from "../domain/network";
import {
  appearance,
  beforeMatch,
  customizerVisible,
  enterArena,
  remoteAppearances,
} from "../state/appearance";
import { createStore } from "../state/store";

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
  /** Optional saved look to lock on join and send in `Hello`. */
  appearance?: () => Appearance | null;
  /** Optional hook called synchronously whenever a match link ends or resets. */
  onReset?: () => void;
};

const TAB_KEY = "arena.match";
const DEVICE_KEY = "arena.lastMatch";
/** Frames held while the engine is not reading (a background tab) before
 * dropping the oldest. The `Welcome` is always kept: it says which seat this is. */
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
  let lockedLook: Appearance | null = null;
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
    // The link is over: a socket that already closed may have left its
    // generation and last frames for the engine. Nobody reads them now, and
    // a later join must not hand them to the engine as the new match's
    // (its final `MatchOver` would end the new match at once).
    generation = 0;
    queue = [];
    ticket = null;
    lockedLook = null;
    deps.onReset?.();
    store(deps.tab, TAB_KEY, null);
    // The seat is gone for good: do not offer it again later. A seat taken
    // by another tab still exists, so it stays on offer ("Play here").
    if (reason !== null && reason !== "Replaced") store(deps.device, DEVICE_KEY, null);
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
    let lastError: string | null = null;
    next.binaryType = "arraybuffer";
    next.onopen = () => {
      opened += 1;
      generation = opened;
      // Frames from an older socket are stale once a new one is open.
      queue = [];
      if (ticket) next.send(encodeHello(ticket.ticket, lockedLook));
      update({ phase: "open", attempt: 0 });
    };
    next.onmessage = (event) => {
      if (!(event.data instanceof ArrayBuffer)) return;
      const frame = new Uint8Array(event.data);
      lastError = serverError(frame) ?? lastError;
      if (isAppearances(frame)) {
        // Only the newest per-seat appearances relay matters; replace any queued older copy.
        const existing = queue.findIndex((queued) => isAppearances(queued));
        if (existing >= 0) queue.splice(existing, 1);
      }
      queue.push(frame);
      if (queue.length > MAX_QUEUED) {
        const oldest = queue.findIndex((queued) => !isWelcome(queued) && !isAppearances(queued));
        queue.splice(Math.max(oldest, 0), 1);
      }
    };
    next.onclose = next.onerror = () => {
      if (socket !== next) return;
      next.onopen = next.onclose = next.onerror = next.onmessage = null;
      socket = null;
      // The generation stays until the engine has read this socket's last
      // frames; a new socket replaces it when it opens.
      if (endsLink(lastError)) end(lastError);
      else retry();
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
      lockedLook = deps.appearance?.() ?? null;
      store(deps.tab, TAB_KEY, next);
      store(deps.device, DEVICE_KEY, next);
      update({ phase: "connecting", attempt: 0, ended: null, status: null });
      connect();
    },
    /** Leaves the match on this tab; the device still offers a rejoin while the match is live. */
    leave() {
      if (state.get().status?.phase === "finished") store(deps.device, DEVICE_KEY, null);
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
    /** The engine's per-frame read: empty without a match. */
    read(): unknown[] {
      if (!ticket) return [];
      const frames = queue;
      queue = [];
      return [generation, ...frames];
    },
    send(frame: Uint8Array) {
      if (socket?.readyState === OPEN) socket.send(frame);
    },
    /** The engine's status report; a final error ends the link. */
    onStatus(raw: unknown) {
      const status = parseStatus(raw);
      if (!status) return;
      update({ status });
      if (status.phase === "closed" && endsLink(status.error)) end(status.error);
    },
  };
}
export type Link = ReturnType<typeof createLink>;

/** Reads a development join link from the address bar and removes it, so
 * the ticket does not linger in the visible URL, history or later shares. */
export function takeJoinFragment(win: Window = window) {
  const ticket = parseJoinFragment(win.location.hash);
  if (win.location.hash.includes("ticket="))
    win.history.replaceState(null, "", win.location.pathname + win.location.search);
  return ticket;
}

/** A join link pasted into an already open tab changes only the fragment,
 * which does not reload the page; join it anyway. */
export function installJoinLinks(link: Link, win: Window = window) {
  const onHash = () => {
    const ticket = takeJoinFragment(win);
    if (ticket) {
      if (beforeMatch.get() || customizerVisible.get()) enterArena(() => {});
      link.join(ticket);
    }
  };
  win.addEventListener("hashchange", onHash);
  return () => win.removeEventListener("hashchange", onHash);
}

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
    appearance: () => appearance.get(),
    onReset: () => remoteAppearances.set(null),
  });
}
