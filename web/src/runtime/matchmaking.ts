// The browser end of queues and parties (ADR 0046, ADR 0047): one WebSocket
// to the front door, opened when the player first queues, forms or joins a
// party, and kept open so the party stays together between matches. The
// player accepts or declines found matches; the seat ticket that follows
// goes to the match link, which joins as it does for a rejoin, so the
// ticket is never shown or put in a URL.

import {
  IDLE_QUEUE,
  guestId,
  isGuestId,
  normalizeCode,
  parseCustomFragment,
  parsePartyFragment,
  parseQueueMessage,
  queueUrl,
  requests,
  type LobbySlot,
  type MatchSettings,
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
  /** Requests made while the connection opens. */
  let pending: string[] = [];
  const update = (patch: Partial<QueueState>) => state.set({ ...state.get(), ...patch });
  const drop = () => {
    if (!socket) return;
    socket.onopen = socket.onclose = socket.onerror = socket.onmessage = null;
    socket.close();
    socket = null;
    pending = [];
  };
  const end = (reason: string) => {
    drop();
    state.set({ ...IDLE_QUEUE, connection: "ended", ended: reason });
  };
  /** Sends `request`, connecting first when needed. */
  const request = (message: string) => {
    if (socket?.readyState === OPEN && state.get().connection === "online") {
      socket.send(message);
      return;
    }
    pending.push(message);
    if (socket) return;
    update({ connection: "connecting", ended: null, notice: null });
    const next = deps.createSocket(deps.url);
    socket = next;
    next.onopen = () => {
      next.send(requests.hello(guest));
      update({ connection: "online" });
      for (const queued of pending) next.send(queued);
      pending = [];
    };
    next.onmessage = (event) => receive(event.data);
    next.onerror = () => {};
    next.onclose = () => {
      if (socket !== next) return;
      socket = null;
      end(state.get().connection === "online" ? "lost" : "unreachable");
    };
  };
  const receive = (data: unknown) => {
    const message = parseQueueMessage(data);
    if (!message) return;
    const current = state.get();
    const now = deps.now();
    switch (message.type) {
      case "party":
        if (message.queue === null)
          update({ party: message.party, phase: "idle", since: null, readyCheck: null });
        else
          update({
            party: message.party,
            mode: message.queue,
            phase: current.phase === "ready" ? "ready" : "queued",
            since: current.since ?? now,
          });
        break;
      case "queued":
        if (current.phase !== "ready")
          update({
            phase: "queued",
            mode: message.mode,
            waiting: message.waiting,
            expectedMs: message.expectedMs,
            // The door's measure, so every party member sees the same time.
            since: now - message.waitedMs,
          });
        break;
      case "ready_check":
        update({
          phase: "ready",
          mode: message.mode,
          notice: null,
          readyCheck: {
            accepted: message.accepted,
            total: message.total,
            you: message.you,
            deadline: now + message.remainingMs,
          },
        });
        break;
      case "requeued":
        update({ phase: "queued", readyCheck: null, notice: "requeued" });
        break;
      case "stopped":
        update({ phase: "idle", since: null, readyCheck: null, notice: message.reason });
        break;
      case "refused":
        update({ notice: message.code });
        break;
      case "match":
        update({ phase: "idle", since: null, readyCheck: null, notice: null });
        deps.onMatch(message.ticket);
        break;
      case "lobby":
        update({ lobby: message.lobby, notice: null });
        break;
      case "lobby_left":
        update({
          lobby: null,
          notice: message.reason === "kicked" ? "kicked" : null,
        });
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
    /** Searches for `mode` (party leader, or on one's own). */
    queue(mode: QueueMode) {
      update({ mode, notice: null });
      request(requests.queue(mode));
    },
    /** Stops the party's search. */
    cancel() {
      if (state.get().phase === "queued") request(requests.cancel());
    },
    /** Accepts the match on offer. */
    accept() {
      const check = state.get().readyCheck;
      if (state.get().phase !== "ready" || !check || check.you) return;
      update({ readyCheck: { ...check, you: true } });
      request(requests.accept());
    },
    /** Declines the match on offer, leaving the queue. */
    decline() {
      if (state.get().phase === "ready") request(requests.decline());
    },
    /** Starts a party with an invite code. */
    createParty() {
      request(requests.createParty());
    },
    /** Joins a friend's party; `false` when the code cannot be one. */
    joinParty(text: string) {
      const code = normalizeCode(text);
      if (!code) {
        update({ notice: "no_such_party" });
        return false;
      }
      request(requests.joinParty(code));
      return true;
    },
    leaveParty() {
      request(requests.leaveParty());
    },
    /** Hosts a new private custom lobby. */
    createLobby(mode: QueueMode = "1v1") {
      update({ notice: null });
      request(requests.createLobby(mode));
    },
    /** Joins a friend's private custom lobby; `false` when the code cannot be one. */
    joinLobby(text: string) {
      const code = normalizeCode(text);
      if (!code) {
        update({ notice: "no_such_lobby" });
        return false;
      }
      update({ notice: null });
      request(requests.joinLobby(code));
      return true;
    },
    leaveLobby() {
      request(requests.leaveLobby());
    },
    setLobbySlot(slot: LobbySlot) {
      request(requests.setLobbySlot(slot));
    },
    moveLobbyMember(member: number, slot: LobbySlot) {
      request(requests.moveLobbyMember(member, slot));
    },
    kickLobbyMember(member: number) {
      request(requests.kickLobbyMember(member));
    },
    transferLobbyHost(member: number) {
      request(requests.transferLobbyHost(member));
    },
    setLobbyMode(mode: QueueMode) {
      request(requests.setLobbyMode(mode));
    },
    setLobbySettings(settings: MatchSettings) {
      request(requests.setLobbySettings(settings));
    },
    startLobby() {
      request(requests.startLobby());
    },
    /** Clears the notice, or an ended connection's message. */
    dismiss() {
      const current = state.get();
      if (current.connection === "ended") state.set(IDLE_QUEUE);
      else update({ notice: null });
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

/** Joins the party (`#party=K7P2QX`) or private custom lobby (`#custom=K7P2QX`)
 * in an invite link on load and when pasted into an open tab, removing the
 * fragment from the address bar immediately. */
export function installPartyLinks(queue: Queue, win: Window = window) {
  const take = () => {
    const hash = win.location.hash;
    const hasParty = hash.includes("party=");
    const hasCustom = hash.includes("custom=");
    if (!hasParty && !hasCustom) return;
    const partyCode = hasParty ? parsePartyFragment(hash) : null;
    const customCode = hasCustom ? parseCustomFragment(hash) : null;
    win.history.replaceState(null, "", win.location.pathname + win.location.search);
    if (customCode) queue.joinLobby(customCode);
    else if (partyCode) queue.joinParty(partyCode);
  };
  take();
  win.addEventListener("hashchange", take);
  return () => win.removeEventListener("hashchange", take);
}
