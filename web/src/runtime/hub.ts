// Entering and leaving the hub (ADR 0057) from the page: the launcher page's
// Enter, the scoreboard's Leave after a match, and the Play and H windows.
// The door places the page in a world and hands over a ticket, which is
// joined like a match (`start.ts`).

import { closeOverlays, returnToLauncher } from "../state/appearance";
import { matchQueue, networkLink } from "../state/network";
import { prematch } from "../state/session";

/** Asks the door for a place in a hub world; `false` without a queue
 * connection to ask on (no door). */
export function enterHub(friend?: string) {
  const queue = matchQueue.get();
  return !!queue && queue.joinHub(friend);
}

/** After a match: back to the hub, or to the launcher page without a door. */
export function returnToHub() {
  if (!matchQueue.get()) {
    returnToLauncher();
    return;
  }
  closeOverlays();
  prematch.set(false);
  enterHub();
}

/** Leaves the hub world (its connection closes) for practice in the arena. */
export function leaveHub() {
  matchQueue.get()?.leaveHub();
  const link = networkLink.get();
  if (link?.state.get().world) link.leave();
}
