import type { Snapshot } from "../bridge/snapshot";
import type { LinkState } from "../domain/network";
import {
  buildMatchScoreboard,
  type MatchScoreboard,
  type ScoreboardRow,
} from "../domain/scoreboard";
import { pendingBinding } from "./binding-capture";
import { customLobbyVisible, customizing, scoreboardVisible, setPanel } from "./session";
import { createStore } from "./store";

/** Retained post-match scoreboard (stays visible after the server sends MatchOver). */
export const postMatchScoreboard = createStore<MatchScoreboard | null>(null);

/** Prevents re-capturing the same finished match after the player clicks Leave. */
let dismissed = false;
let lastCapturedSnapshot: Snapshot | null = null;

function rowsEqual(a: readonly ScoreboardRow[], b: readonly ScoreboardRow[]): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i += 1) {
    const left = a[i];
    const right = b[i];
    if (
      left.actor !== right.actor ||
      left.tag !== right.tag ||
      left.name !== right.name ||
      left.damageDone !== right.damageDone ||
      left.damageTaken !== right.damageTaken ||
      left.healingDone !== right.healingDone ||
      left.healingReceived !== right.healingReceived ||
      left.killingBlows !== right.killingBlows ||
      left.disconnected !== right.disconnected
    ) {
      return false;
    }
  }
  return true;
}

function boardsEqual(a: MatchScoreboard | null, b: MatchScoreboard): boolean {
  return (
    a !== null &&
    a.result === b.result &&
    a.durationTicks === b.durationTicks &&
    a.tickRate === b.tickRate &&
    rowsEqual(a.allies, b.allies) &&
    rowsEqual(a.opponents, b.opponents)
  );
}

/** Closes transient combat modals/editors so they never trap focus under the scoreboard. */
function closeTransientModes() {
  setPanel(null);
  pendingBinding.set(null);
  customizing.set(false);
  customLobbyVisible.set(false);
}

export function setPostMatchScoreboard(board: MatchScoreboard | null) {
  if (board !== null) closeTransientModes();
  postMatchScoreboard.set(board);
  scoreboardVisible.set(board !== null);
}

/** Clears the retained scoreboard and rearms capture for the next match. */
export function clearPostMatchScoreboard() {
  dismissed = false;
  lastCapturedSnapshot = null;
  if (postMatchScoreboard.get() !== null || scoreboardVisible.get()) {
    setPostMatchScoreboard(null);
  }
}

/** Dismisses the current finished match's scoreboard until a new match or practice snapshot arrives. */
export function dismissPostMatchScoreboard() {
  dismissed = true;
  lastCapturedSnapshot = null;
  if (postMatchScoreboard.get() !== null || scoreboardVisible.get()) {
    setPostMatchScoreboard(null);
  }
}

/** Canonical scoreboard lifecycle sync used by both runtime (`start.ts`) and `<Scoreboard />`. */
export function syncPostMatchScoreboard(
  snapshot: Snapshot | null,
  linkState: LinkState | null,
): MatchScoreboard | null {
  const statusPhase = linkState?.status?.phase ?? null;
  if (
    linkState?.phase === "connecting" ||
    statusPhase === "waiting" ||
    statusPhase === "playing" ||
    (snapshot?.networked && !snapshot.won)
  ) {
    clearPostMatchScoreboard();
    return null;
  }

  if (!snapshot?.networked || !snapshot.won) {
    dismissed = false;
    lastCapturedSnapshot = null;
    return postMatchScoreboard.get();
  }

  // With an active match link, only capture once the match is actually finished (never from
  // stale Match 1 snapshots lingering while Match 2's socket opens before Welcome/Snapshot).
  const canCapture =
    linkState === null ||
    statusPhase === "finished" ||
    (linkState.phase === "ended" && linkState.ended === "MatchOver") ||
    (statusPhase === "closed" && linkState.status?.error === "MatchOver");
  if (!canCapture || dismissed || !snapshot.roundResult || !snapshot.scoreboard) {
    return postMatchScoreboard.get();
  }

  if (snapshot === lastCapturedSnapshot && postMatchScoreboard.get() !== null) {
    return postMatchScoreboard.get();
  }

  const board = buildMatchScoreboard(snapshot);
  if (!board) return postMatchScoreboard.get();
  lastCapturedSnapshot = snapshot;
  if (!boardsEqual(postMatchScoreboard.get(), board)) {
    setPostMatchScoreboard(board);
  }
  return postMatchScoreboard.get();
}
