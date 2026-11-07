import type { Snapshot } from "../../bridge/snapshot";
export const PULSE = {
  id: "pulse" as const,
  name: "Pulse",
  description:
    "Release a focused burst at your selected opponent. Requires range and a clear line of sight.",
};
export function isEngineReady(state: Snapshot | null): state is Snapshot {
  return (
    !!state &&
    state.frames > 10 &&
    ["ready", "failed", "placeholder"].includes(state.character.status)
  );
}
export function availability(state: Snapshot | null): string {
  return !isEngineReady(state)
    ? "Loading arena"
    : state.won
      ? "Round complete"
      : !state.selected
        ? "Select a target"
        : state.cooldown > 0
          ? `Recharging · ${state.cooldown.toFixed(1)} s`
          : state.attackStatus;
}
export function canActivate(state: Snapshot | null) {
  return availability(state) === "Ready";
}
