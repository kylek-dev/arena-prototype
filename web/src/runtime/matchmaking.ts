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
  MAX_CHALLENGES,
  CHALLENGE_ONLY_REFUSALS,
  CHALLENGE_REFUSALS,
  CHALLENGE_TTL_MS,
  type Invite,
  type OwnChallenge,
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
import { HUB_REFUSALS, OUT_OF_HUB, hubRequests, type HubState } from "../domain/hub";

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
  /** The door took this page out of its hub world (the world connection
   * ended, or it never connected): close the world link. */
  onWorldLeft?(): void;
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
  let lastAsked: "chat" | "hub" | "challenge" | "other" = "other";
  /** The last challenge request (ADR 0060), for its refusal: sending one
   * (with the challenge it would replace) or answering one. */
  let challengeAsked:
    | { kind: "send"; with: string; previous: OwnChallenge | null }
    | { kind: "answer"; with: string }
    | null = null;
  const sameHandle = (a: string, b: string) => a.toLowerCase() === b.toLowerCase();
  /** Out of the hub, or matched: the door has closed this player's
   * challenges, telling only the other players. */
  const forgetChallenges = () => {
    challengeAsked = null;
    update({ challenges: [], challenging: null });
  };
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
  // The hub (ADR 0057): whether the door has placed this page in a world.
  const hub = createStore<HubState>(OUT_OF_HUB);
  const drop = () => {
    if (!socket) return;
    socket.onopen = socket.onclose = socket.onerror = socket.onmessage = null;
    socket.close();
    socket = null;
    pending = [];
  };
  const end = (reason: string) => {
    drop();
    challengeAsked = null;
    // A listening connection ends quietly: offline, with the reason kept for
    // whoever reopens it (`runtime/social.ts`).
    state.set({ ...IDLE_QUEUE, connection: background ? "offline" : "ended", ended: reason });
    background = false;
    // The door forgets a closed connection's hub place.
    if (hub.get().status !== "out") hub.set({ ...OUT_OF_HUB, notice: "lost" });
  };
  /** Sends `request`, connecting first when needed. */
  const request = (
    message: string | null,
    kind: "chat" | "hub" | "challenge" | "other" = "other",
  ) => {
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
        // Answers to a hub join; a stray `not_in_hub` (a move just after
        // leaving) is no news.
        // Answers to the hub join: its own codes, and any refusal while it
        // was the last request (a missing name, the rate limit).
        if (
          hub.get().status === "joining" &&
          (HUB_REFUSALS.has(message.code) || lastAsked === "hub")
        ) {
          hub.set({ ...OUT_OF_HUB, notice: message.code });
          break;
        }
        // Replayed ignores' refusals first: they are never news.
        if (IGNORE_REFUSALS.has(message.code) && replaying > 0 && !ignoring) {
          replaying -= 1;
          break;
        }
        // A challenge's refusal is told with the challenge (ADR 0060): the
        // last request was one, or the code belongs to challenges alone.
        if (
          challengeAsked &&
          CHALLENGE_REFUSALS.has(message.code) &&
          (lastAsked === "challenge" || CHALLENGE_ONLY_REFUSALS.has(message.code))
        ) {
          const asked = challengeAsked;
          challengeAsked = null;
          const notice = { with: asked.with, code: message.code, mine: asked.kind === "send" };
          if (asked.kind === "send")
            // Refused: nothing changed, so an earlier challenge still stands.
            update({ challenging: asked.previous, challengeNotice: notice });
          else
            update({
              challenges:
                message.code === "challenge_gone"
                  ? current.challenges.filter((c) => !sameHandle(c.from, asked.with))
                  : current.challenges.map((c) =>
                      sameHandle(c.from, asked.with) ? { ...c, answering: false } : c,
                    ),
              challengeNotice: notice,
            });
          break;
        }
        if (message.code === "not_in_hub" && lastAsked !== "chat") break;
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
        challengeAsked = null;
        update({
          phase: "idle",
          since: null,
          readyCheck: null,
          notice: null,
          // Matched: the door closed every challenge of this player's.
          challenges: [],
          challenging: null,
          challengeNotice: null,
        });
        deps.onMatch(message.ticket);
        break;
      case "world":
        // Joined like a match: the world server runs the hub (ADR 0057).
        if (hub.get().status === "out") break;
        hub.set({ status: "in", instance: message.instance, notice: null });
        deps.onMatch(message.ticket);
        break;
      case "hub_left":
        hub.set({ ...OUT_OF_HUB, notice: message.reason });
        forgetChallenges();
        // A new name needs a new place: ask again.
        if (message.reason === "renamed") api.joinHub();
        else if (message.reason === "left") deps.onWorldLeft?.();
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
      case "challenged": {
        // One card per challenger, the newest kept; a few at most.
        const others = current.challenges.filter((c) => !sameHandle(c.from, message.from));
        const challenge = {
          from: message.from,
          deadline: now + message.remainingMs,
          answering: false,
        };
        update({ challenges: [...others, challenge].slice(-MAX_CHALLENGES) });
        break;
      }
      case "challenge_sent":
        challengeAsked = null;
        update({
          challenging: { to: message.to, sent: true, deadline: now + CHALLENGE_TTL_MS },
          challengeNotice: null,
        });
        break;
      case "challenge_ended": {
        const held = current.challenges.find((c) => sameHandle(c.from, message.with));
        const own =
          current.challenging?.sent && sameHandle(current.challenging.to, message.with)
            ? current.challenging
            : null;
        // Which challenge ended: `declined` and `left` reach only the
        // challenger, `withdrawn` only the challenged player, `busy` the
        // challenged player only after accepting; `expired` reaches both,
        // so with challenges both ways it is the one due first.
        let mine: boolean;
        switch (message.reason) {
          case "declined":
          case "left":
            mine = true;
            break;
          case "withdrawn":
            mine = false;
            break;
          case "busy":
            mine = !held?.answering;
            break;
          case "expired":
            mine = !held || (!!own && own.deadline <= held.deadline);
            break;
        }
        if (mine && own) {
          update({
            challenging: null,
            challengeNotice: { with: message.with, code: message.reason, mine: true },
          });
        } else if (!mine && held) {
          update({
            challenges: current.challenges.filter((c) => c !== held),
            // A held card going away is news only when it was accepted.
            challengeNotice: held.answering
              ? { with: message.with, code: message.reason, mine: false }
              : current.challengeNotice,
          });
        }
        break;
      }
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
    /** Whether the door has placed this page in a hub world (ADR 0057). */
    hub,
    /** Asks the door for a place in a hub world, beside a friend or party
     * member when named; the `world` ticket it answers is joined like a
     * match (`onMatch`). The look goes to the world server in `Hello`. */
    joinHub(friend?: string) {
      if (hub.get().status !== "out") return true;
      hub.set({ ...OUT_OF_HUB, status: "joining" });
      // Entering the game asks for the hub on the way: a door that cannot
      // be reached (local practice without one) is no news.
      const quiet = !socket;
      request(hubRequests.join(friend), "hub");
      if (quiet) background = true;
      return true;
    },
    leaveHub() {
      if (hub.get().status === "out") return;
      hub.set(OUT_OF_HUB);
      forgetChallenges();
      request(hubRequests.leave());
    },
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
    /** Challenges `handle` in this hub world to a 1v1 (ADR 0060); a new
     * challenge replaces one's own open one. */
    challenge(handle: string) {
      const current = state.get();
      // A challenge still on its way is replaced, as the door replaces it.
      const previous = current.challenging?.sent ? current.challenging : null;
      challengeAsked = { kind: "send", with: handle, previous };
      update({ challenging: { to: handle, sent: false, deadline: 0 }, challengeNotice: null });
      request(requests.challenge(handle), "challenge");
    },
    /** Accepts (a match follows) or declines another player's challenge. */
    answerChallenge(from: string, accept: boolean) {
      const current = state.get();
      const held = current.challenges.find((c) => sameHandle(c.from, from));
      if (!held || held.answering) return;
      challengeAsked = { kind: "answer", with: from };
      update({
        challenges: accept
          ? current.challenges.map((c) => (c === held ? { ...c, answering: true } : c))
          : current.challenges.filter((c) => c !== held),
        challengeNotice: null,
      });
      request(requests.answerChallenge(held.from, accept), "challenge");
    },
    /** Takes back one's own challenge. */
    withdrawChallenge() {
      if (!state.get().challenging?.sent) return;
      update({ challenging: null });
      request(requests.withdrawChallenge());
    },
    dismissChallengeNotice() {
      update({ challengeNotice: null });
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

export function browserQueue(onMatch: (ticket: MatchTicket) => void, onWorldLeft?: () => void) {
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
    onWorldLeft,
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
