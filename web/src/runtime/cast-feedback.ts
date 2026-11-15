import type { Snapshot } from "../bridge/snapshot";
import { createStore } from "../state/store";

export const castError = createStore<{ message: string } | null>(null);
let pending: Snapshot | null = null;

export function rejectCast(message: string) {
  pending = null;
  castError.set({ message });
}

export function beginCast(state: Snapshot) {
  castError.set(null);
  pending = state;
}

export function clearCastFeedback() {
  pending = null;
  castError.set(null);
}

// Observe only submitted attempts, never turn passive availability into an error.
export function observeCastResult(next: Snapshot) {
  if (!pending) return;
  if (next.tick < pending.tick) {
    clearCastFeedback();
    return;
  }
  if (next.tick === pending.tick) return;
  const previous = pending;
  pending = null;
  if (next.cooldown > previous.cooldown || next.target.health < previous.target.health) return;
  const rejectedFeedback = [
    "Select a target first",
    "Out of range · move closer",
    "Pulse is recharging",
    "Line of sight blocked by pillar",
    "Round complete · reset to play again",
    "Command rejected",
  ];
  if (next.feedback !== previous.feedback && rejectedFeedback.includes(next.feedback)) {
    rejectCast(next.feedback);
  } else if (next.attackStatus !== "Ready") {
    rejectCast(next.attackStatus);
  }
}
