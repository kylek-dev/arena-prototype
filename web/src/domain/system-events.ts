// System lines for the chat box (planning/issues/chat.md): what changed, in
// words, from one state of the queue, friends list or match to the next.
// Only the page writes these; the door never sends a `system` line. Pure, so
// each event is unit tested without sockets.

import type { Entity } from "./units/selection";
import type { FriendList } from "./friends";
import { memberName, type CustomLobby, type Party } from "./matchmaking";
import type { RoundResult } from "../bridge/snapshot";
import { roundResultLabel } from "./round-clock";

/** Joining, leaving, hosting and being removed from a custom lobby. */
export function lobbyEvents(
  before: CustomLobby | null,
  after: CustomLobby | null,
  kicked: boolean,
): string[] {
  if (!before && !after) return [];
  if (!before && after) return [`You joined custom lobby ${after.code}.`];
  if (before && !after)
    return [kicked ? "You were removed from the custom lobby." : "You left the custom lobby."];
  if (!before || !after) return [];
  if (before.code !== after.code) return [`You joined custom lobby ${after.code}.`];
  const lines: string[] = [];
  const had = new Map(before.members.map((member) => [member.id, member]));
  const has = new Set(after.members.map((member) => member.id));
  for (const member of after.members)
    if (!had.has(member.id)) lines.push(`${memberName(member)} joined the lobby.`);
  for (const member of before.members)
    if (!has.has(member.id)) lines.push(`${memberName(member)} left the lobby.`);
  if (!before.host && after.host) lines.push("You are now the lobby host.");
  return lines;
}

/** Players joining or leaving this player's party. */
export function partyEvents(before: Party | null, after: Party | null): string[] {
  if (before?.code && !after?.code) return ["You left the party."];
  if (!after?.code) return [];
  if (!before?.code || before.code !== after.code)
    return after.members > 1 ? [`You joined party ${after.code}.`] : [];
  if (after.members > before.members) return ["A player joined your party."];
  if (after.members < before.members) return ["A player left your party."];
  if (!before.leader && after.leader) return ["You now lead the party."];
  return [];
}

/** Friends coming online, and new friend requests. Nothing for the first
 * list (everyone already online is not news). */
export function friendEvents(before: FriendList | null, after: FriendList | null): string[] {
  if (!before || !after) return [];
  const was = new Map(before.friends.map((friend) => [friend.handle, friend]));
  const lines: string[] = [];
  for (const friend of after.friends) {
    const old = was.get(friend.handle);
    if (friend.relation === "incoming" && old?.relation !== "incoming")
      lines.push(`${friend.handle} wants to be friends.`);
    else if (
      friend.relation === "friend" &&
      friend.presence !== "offline" &&
      old?.relation === "friend" &&
      old.presence === "offline"
    )
      lines.push(`${friend.handle} is online.`);
    else if (friend.relation === "friend" && old?.relation === "outgoing")
      lines.push(`${friend.handle} accepted your friend request.`);
  }
  return lines;
}

/** Fighters disconnecting and reconnecting in a network match. */
export function rosterEvents(
  before: readonly Entity[] | undefined,
  after: readonly Entity[] | undefined,
): string[] {
  if (!before || !after || before.length !== after.length) return [];
  const lines: string[] = [];
  after.forEach((unit, index) => {
    const old = before[index];
    if (!old.disconnected && unit.disconnected) lines.push(`${unit.name} disconnected.`);
    else if (old.disconnected && !unit.disconnected) lines.push(`${unit.name} reconnected.`);
  });
  return lines;
}

/** The round starting and its result. */
export function roundEvents(
  before: { playing: boolean; result: RoundResult | null },
  after: { playing: boolean; result: RoundResult | null },
  spectating: boolean,
): string[] {
  if (!before.playing && after.playing) return ["The round has started."];
  if (!before.result && after.result) {
    const label = roundResultLabel(after.result, spectating);
    return label ? [`Round over: ${label}.`] : [];
  }
  return [];
}
