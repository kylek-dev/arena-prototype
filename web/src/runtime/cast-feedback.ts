import type { Snapshot } from "../bridge/snapshot";
import type { AbilityId } from "../domain/action-bars/model";
import { spellCooldown } from "../domain/abilities/catalog";
import { createStore } from "../state/store";

export const castError = createStore<{ message: string } | null>(null);
let pending: { snapshot: Snapshot; ability: AbilityId } | null = null;

export function rejectCast(message: string) {
  pending = null;
  castError.set({ message });
}

export function beginCast(state: Snapshot, ability: AbilityId = "pulse") {
  castError.set(null);
  pending = { snapshot: state, ability };
}

export function clearCastFeedback() {
  pending = null;
  castError.set(null);
}

// Observe only submitted attempts, never turn passive availability into an error.
export function observeCastResult(next: Snapshot) {
  if (!pending) return;
  if (next.tick < pending.snapshot.tick) {
    clearCastFeedback();
    return;
  }
  if (next.tick === pending.snapshot.tick) return;
  const { snapshot: previous, ability } = pending;
  // Held by the cast buffer: keep watching until it fires or fails.
  if (next.player.buffered === ability) {
    pending = { snapshot: next, ability };
    return;
  }
  pending = null;
  if (
    spellCooldown(next, ability) > spellCooldown(previous, ability) ||
    next.player.cast?.spell === ability ||
    next.target.health < previous.target.health
  )
    return;
  const rejectedFeedback = [
    "Select a target first",
    "Out of range · move closer",
    "Pulse is recharging",
    "Global cooldown active",
    "Already casting",
    "Not enough resource",
    "Stand still to cast",
    "Casting locked out",
    "Line of sight blocked by pillar",
    "Round complete · reset to play again",
    "You are defeated · your team fights on",
    "Command rejected",
  ];
  const status = next.spells?.[ability].status ?? next.attackStatus;
  if (next.feedback !== previous.feedback && rejectedFeedback.includes(next.feedback)) {
    rejectCast(next.feedback);
  } else if (status !== "Ready") {
    rejectCast(status);
  }
}
