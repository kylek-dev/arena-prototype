// Friends and invites (social-identity step 4): what the front door's
// `/api/friends` answers and how the interface words it. Players are
// addressed by `Name#number` only. Pure, so it is unit tested without a
// server.

import { isHandle } from "./handles";

export type Presence = "offline" | "online" | "searching" | "in_lobby" | "in_match";
export type Relation = "friend" | "incoming" | "outgoing";
export type Friend = {
  handle: string;
  relation: Relation;
  /** Friends only; `null` for requests. */
  presence: Presence | null;
};
export type FriendList = { friends: Friend[]; blocked: string[] };
export type FriendAction = "request" | "accept" | "decline" | "remove" | "block" | "unblock";

const PRESENCE = new Set<Presence>(["offline", "online", "searching", "in_lobby", "in_match"]);
const RELATION = new Set<Relation>(["friend", "incoming", "outgoing"]);
/** The door caps 200 friends and 50 waiting requests each way; blocks are
 * bounded here so a broken answer cannot flood the page. */
const MAX_ENTRIES = 400;

/** Parses `GET /api/friends`; `null` when it is not understood. */
export function parseFriends(raw: unknown): FriendList | null {
  if (!raw || typeof raw !== "object") return null;
  const v = raw as Record<string, unknown>;
  if (!Array.isArray(v.friends) || !Array.isArray(v.blocked)) return null;
  if (v.friends.length > MAX_ENTRIES || v.blocked.length > MAX_ENTRIES) return null;
  const friends: Friend[] = [];
  for (const entry of v.friends) {
    if (!entry || typeof entry !== "object") return null;
    const f = entry as Record<string, unknown>;
    if (!isHandle(f.handle) || !RELATION.has(f.relation as Relation)) return null;
    const relation = f.relation as Relation;
    if (relation === "friend" && !PRESENCE.has(f.presence as Presence)) return null;
    friends.push({
      handle: f.handle,
      relation,
      presence: relation === "friend" ? (f.presence as Presence) : null,
    });
  }
  if (!v.blocked.every(isHandle)) return null;
  return { friends, blocked: v.blocked as string[] };
}

const ORDER: Record<Presence, number> = {
  in_lobby: 0,
  searching: 0,
  online: 0,
  in_match: 1,
  offline: 2,
};
/** Friends grouped for the panel: requests to answer, friends (online
 * first, then by name), and requests waiting on the other player. */
export function groupFriends(list: FriendList) {
  const byName = (a: Friend, b: Friend) =>
    a.handle.localeCompare(b.handle, "en", { sensitivity: "base" });
  return {
    incoming: list.friends.filter((f) => f.relation === "incoming").sort(byName),
    friends: list.friends
      .filter((f) => f.relation === "friend")
      .sort(
        (a, b) => ORDER[a.presence ?? "offline"] - ORDER[b.presence ?? "offline"] || byName(a, b),
      ),
    outgoing: list.friends.filter((f) => f.relation === "outgoing").sort(byName),
  };
}

const PRESENCE_LABELS: Record<Presence, string> = {
  offline: "Offline",
  online: "Online",
  searching: "In a queue",
  in_lobby: "In a custom lobby",
  in_match: "In a match",
};
export function presenceLabel(presence: Presence) {
  return PRESENCE_LABELS[presence];
}
/** Whether a friend can be invited now (the door still decides). */
export function invitable(friend: Friend) {
  return friend.relation === "friend" && friend.presence !== "offline";
}

/** A typed `Name#number`, trimmed; `null` when it cannot be one.
 * Letter case is kept: the door compares names without case. */
export function normalizeHandle(text: string) {
  const handle = text.trim();
  return isHandle(handle) ? handle : null;
}

const FRIEND_ERRORS: Record<string, string> = {
  no_such_player: "No player has that name and number.",
  yourself: "You cannot add yourself as a friend.",
  bad_request: "Enter the player's full name and four-digit number.",
  already_friends: "You are already friends with this player.",
  already_asked: "A friend request to this player is already pending.",
  too_many: "The friends list or request limit has been reached. Remove an entry and try again.",
  nothing_there: "This request or friend is no longer available.",
  pick_a_name_first: "Choose a player name before adding friends.",
  not_logged_in: "Log in to use the friends list.",
  rate_limited: "Too many requests. Please wait a moment and try again.",
  forbidden: "This page cannot change your friends list. Open the game from its own address.",
  offline: "The server could not be reached. Check your connection and try again.",
  failed: "The server could not complete the request. Please try again later.",
};
/** Words for a friend action the door refused. */
export function friendErrorText(code: string) {
  return FRIEND_ERRORS[code] ?? FRIEND_ERRORS.failed;
}

/** What a friend action did, for a short confirmation. */
export function friendDoneText(action: FriendAction, handle: string, relation?: Relation) {
  switch (action) {
    case "request":
      return relation === "friend"
        ? `You and ${handle} are now friends.`
        : `Friend request sent to ${handle}.`;
    case "accept":
      return `You and ${handle} are now friends.`;
    case "decline":
      return `Friend request from ${handle} declined.`;
    case "remove":
      return `${handle} was removed from your friends list.`;
    case "block":
      return `${handle} is blocked and can no longer send you requests or invitations.`;
    case "unblock":
      return `${handle} is no longer blocked.`;
  }
}
