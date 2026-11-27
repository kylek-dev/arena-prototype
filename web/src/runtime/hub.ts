// Entering and leaving the hub (ADR 0056) from the page: the launcher page's
// Enter, the scoreboard's Leave after a match, and the Play window.

import { appearance, closeOverlays, returnToLauncher } from "../state/appearance";
import { matchQueue } from "../state/network";
import { prematch } from "../state/session";

/** Asks the door for a hub place with the saved look; `false` without a
 * queue connection to ask on (no door). */
export function enterHub(friend?: string) {
  const queue = matchQueue.get();
  return !!queue && queue.joinHub(appearance.get(), friend);
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

/** Leaves the hub for practice in the arena. */
export function leaveHub() {
  matchQueue.get()?.leaveHub();
}
