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
  MAX_INVITES,
  type Invite,
  type MatchSettings,
  type QueueMode,
  type QueueState,
} from "../domain/matchmaking";
import type { MatchTicket } from "../domain/network";
import {
  CHAT_ONLY_REFUSALS,
  EMPTY_CHAT,
  IGNORE_REFUSALS,
  MAX_IGNORE_REPLAY,
  SHARED_CHAT_REFUSALS,
  receiveChat,
  closeConversation,
  openConversation,
  showChatTab,
  systemChat,
  withIgnored,
  type ChatChannel,
  type ChatState,
  type ChatTab,
} from "../domain/chat";
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
  /** Chat apart from the queue state, so a busy channel re-renders only
   * the chat box. Kept across connections. */
  const chat = createStore<ChatState>(EMPTY_CHAT);
  const updateChat = (patch: Partial<ChatState>) => chat.set({ ...chat.get(), ...patch });
  /** What the last request was, to tell whose refusal arrives next. */
  let lastAsked: "chat" | "other" = "other";
  /** An ignore waiting on the door (it answers only refusals). */
  let ignoring: string | null = null;
  let pendingIgnore: string | null = null;
  /** Replayed ignores whose refusals are still to come (silently). */
  let replaying = 0;
  const guest = loadGuest(deps);
  let socket: TextSocket | null = null;
  /** Requests made while the connection opens. */
  let pending: string[] = [];
  /** Opens the connection with nothing to send, to hear invites, chat and
   * friend changes (logged-in players). */
  const open = () => {
    if (socket) return;
    background = true;
    request(null);
  };
  /** Whether the connection was opened only to listen, with nothing asked
   * of it yet: its end is not news to the player (no "lost" bar). */
  let background = false;
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
    // A listening connection ends quietly: offline, with the reason kept for
    // whoever reopens it (`runtime/social.ts`).
    state.set({ ...IDLE_QUEUE, connection: background ? "offline" : "ended", ended: reason });
    background = false;
  };
  /** Sends `request`, connecting first when needed. */
  const request = (message: string | null, kind: "chat" | "other" = "other") => {
    if (message) {
      lastAsked = kind;
      // The door answers an ignore before any later request of the same
      // connection reaches its matchmaker: once another request goes out,
      // an ignore refusal is no longer expected for the earlier one.
      if (message !== pendingIgnore) ignoring = null;
    }
    if (socket?.readyState === OPEN && state.get().connection === "online") {
      if (message) {
        socket.send(message);
        background = false;
      }
      return;
    }
    if (message) {
      pending.push(message);
      background = false;
    }
    if (socket) return;
    update({ connection: "connecting", ended: null, notice: null });
    const next = deps.createSocket(deps.url);
    socket = next;
    next.onopen = () => {
      next.send(requests.hello(guest));
      update({ connection: "online" });
      // An ignore sent on the old connection gets no answer here; one still
      // waiting to be sent does, and comes before the replays below.
      if (!pendingIgnore || !pending.includes(pendingIgnore)) ignoring = pendingIgnore = null;
      for (const queued of pending) next.send(queued);
      pending = [];
      // The door keeps ignores per connection: tell a new one the newest few
      // again (the chat box hides the rest itself). Their refusals, such as
      // an ignored player who is offline now, are expected and stay quiet.
      const replay = chat.get().ignored.slice(-MAX_IGNORE_REPLAY);
      replaying = replay.length;
      for (const handle of replay) next.send(requests.ignore(handle, true));
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
        if (current.joining && message.party.code === current.joining.code)
          update({ joining: null });
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
        if (IGNORE_REFUSALS.has(message.code) && replaying > 0 && !ignoring) {
          replaying -= 1;
          break;
        }
        if (
          CHAT_ONLY_REFUSALS.has(message.code) ||
          (lastAsked === "chat" && SHARED_CHAT_REFUSALS.has(message.code))
        ) {
          const failed = IGNORE_REFUSALS.has(message.code) ? ignoring : null;
          if (failed) ignoring = null;
          const log = chat.get();
          chat.set({
            ...(failed ? withIgnored(log, failed, false) : log),
            error: message.code,
          });
          break;
        }
        // An accepted invite that could not be joined is told at the prompt.
        if (current.joining)
          update({
            joining: null,
            inviteError: { from: current.joining.from, code: message.code },
          });
        else update({ notice: message.code, inviting: null });
        break;
      case "match":
        update({ phase: "idle", since: null, readyCheck: null, notice: null });
        deps.onMatch(message.ticket);
        break;
      case "lobby":
        update({ lobby: message.lobby, notice: null, joining: null });
        break;
      case "lobby_left":
        update({
          lobby: null,
          notice: message.reason === "kicked" ? "kicked" : null,
        });
        break;
      case "invite": {
        // One prompt per friend, the newest kept; a few at most.
        const others = current.invites.filter((invite) => invite.from !== message.invite.from);
        update({ invites: [...others, message.invite].slice(-MAX_INVITES) });
        break;
      }
      case "invite_sent":
        update({ inviteSent: message.to, inviting: null, notice: null });
        break;
      case "friends_changed":
        update({ friendsChanged: current.friendsChanged + 1 });
        break;
      case "chat":
        chat.set(receiveChat(chat.get(), message.line));
        break;
      case "error":
        end(message.code);
        break;
    }
  };
  const forget = (invite: Invite) =>
    update({ invites: state.get().invites.filter((other) => other !== invite) });

  const api = {
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
    /** Opens the door connection now, if it is not open (logged-in players
     * hear invites and friend changes without queueing first). */
    connect: open,
    /** Invites a friend to this player's custom lobby, else party. */
    invite(handle: string) {
      if (state.get().inviting) return;
      update({ notice: null, inviteSent: null, inviting: handle });
      request(requests.invite(handle));
    },
    /** Clears "invite sent". */
    dismissInviteSent() {
      update({ inviteSent: null });
    },
    /** Chat: the log, tabs and last refusal. */
    chat,
    /** Posts `text` to a channel (the door names the sender). */
    say(channel: Exclude<ChatChannel, "whisper">, text: string) {
      updateChat({ error: null });
      request(requests.chat(channel, text), "chat");
    },
    /** Whispers to a friend. */
    whisper(to: string, text: string) {
      // `/r` answers the last conversation, whoever began it.
      updateChat({ error: null, whisperTo: to });
      request(requests.whisper(to, text), "chat");
    },
    /** Stops (or resumes) hearing a player on this connection. */
    ignore(handle: string, on: boolean) {
      const message = requests.ignore(handle, on);
      ignoring = on ? handle : null;
      pendingIgnore = message;
      const log = withIgnored(chat.get(), handle, on);
      // Ignoring a player also ends the conversation with them.
      chat.set(
        on
          ? {
              ...closeConversation(log, handle),
              whisperTo:
                log.whisperTo?.toLowerCase() === handle.toLowerCase() ? null : log.whisperTo,
              error: null,
            }
          : { ...log, error: null },
      );
      request(message, "chat");
    },
    /** Adds a system line (written by the page, never sent). */
    system(text: string) {
      chat.set(systemChat(chat.get(), text));
    },
    /** Empties the chat log (ignores stay). */
    clearChat() {
      chat.set({ ...chat.get(), messages: [], unread: {} });
    },
    showChat(tab: ChatTab) {
      chat.set(showChatTab(chat.get(), tab));
    },
    /** Opens the conversation tab with a player. */
    openConversation(handle: string) {
      chat.set(openConversation(chat.get(), handle));
    },
    closeConversation(handle: string) {
      chat.set(closeConversation(chat.get(), handle));
    },
    dismissChatError() {
      updateChat({ error: null });
    },
    /** Clears an invite that could not be joined. */
    dismissInviteError() {
      update({ inviteError: null });
    },
    /** Joins the party or lobby an invite is for. */
    acceptInvite(invite: Invite) {
      forget(invite);
      update({ joining: invite, inviteError: null });
      if (invite.kind === "lobby") api.joinLobby(invite.code);
      else api.joinParty(invite.code);
    },
    /** Declining is doing nothing: the prompt goes. */
    declineInvite: forget,
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
  return api;
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
