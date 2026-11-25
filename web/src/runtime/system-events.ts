// Writes system lines into the chat box as the queue, friends list and
// match change (planning/issues/chat.md "System messages"). The words come
// from `domain/system-events.ts`; this only watches the stores.

import type { Snapshot } from "../bridge/snapshot";
import type { LinkState } from "../domain/network";
import {
  friendEvents,
  lobbyEvents,
  partyEvents,
  rosterEvents,
  roundEvents,
} from "../domain/system-events";
import type { Friends } from "./friends";
import type { Queue } from "./matchmaking";

type Store<T> = { get(): T; subscribe(listener: () => void): () => void };

export function installSystemEvents(
  queue: Pick<Queue, "state" | "system">,
  friends: Pick<Friends, "state">,
  engine: Store<Snapshot | null>,
  link: Store<LinkState> | null,
) {
  const say = (lines: string[]) => {
    for (const line of lines) queue.system(line);
  };
  let queueBefore = queue.state.get();
  const onQueue = () => {
    const after = queue.state.get();
    const before = queueBefore;
    // Only while connected: a dropped connection resets the state, which is
    // not the player leaving, and a new one starts from what the door says.
    if (after.connection !== "online") {
      if (before.connection === "online") queueBefore = after;
      return;
    }
    queueBefore = after;
    if (before.connection !== "online") return;
    say([
      ...(after.phase === "ready" && before.phase !== "ready"
        ? ["Match found: accept it in the window."]
        : []),
      ...lobbyEvents(before.lobby, after.lobby, after.notice === "kicked"),
      ...partyEvents(before.party, after.party),
    ]);
  };
  let listBefore = friends.state.get().list;
  const onFriends = () => {
    const after = friends.state.get().list;
    const before = listBefore;
    listBefore = after;
    say(friendEvents(before, after));
  };
  const round = () => {
    const snapshot = engine.get();
    const networked = !!snapshot?.networked;
    return {
      phase: link?.get().status?.phase ?? null,
      result: networked ? (snapshot?.roundResult ?? null) : null,
    };
  };
  let roundBefore = round();
  let engineBefore = engine.get();
  const onMatch = () => {
    const after = engine.get();
    const before = engineBefore;
    engineBefore = after;
    const roundAfter = round();
    // A round starts when waiting gives way to play; reconnecting to a
    // round already under way is not a start.
    const lines = roundEvents(
      {
        playing: roundBefore.phase !== "waiting",
        result: roundBefore.result,
      },
      { playing: roundAfter.phase === "playing", result: roundAfter.result },
      !!after?.spectating,
    );
    roundBefore = roundAfter;
    if (before?.networked && after?.networked)
      lines.push(...rosterEvents(before.roster, after.roster));
    say(lines);
  };
  const stops = [
    queue.state.subscribe(onQueue),
    friends.state.subscribe(onFriends),
    engine.subscribe(onMatch),
    ...(link ? [link.subscribe(onMatch)] : []),
  ];
  return () => {
    for (const stop of stops) stop();
  };
}
