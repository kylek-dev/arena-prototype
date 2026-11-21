import type { Snapshot } from "../../bridge/snapshot";
import type { AbilityId } from "../action-bars/model";

export type SpellMeta = {
  id: AbilityId;
  name: string;
  symbol: string;
  description: string;
  friendly?: boolean;
};

export const SPELLS: Record<AbilityId, SpellMeta> = {
  pulse: {
    id: "pulse",
    name: "Pulse",
    symbol: "ϟ",
    description:
      "Release a focused burst at your selected opponent. Instant, usable while moving, requires range and line of sight.",
  },
  rift_bolt: {
    id: "rift_bolt",
    name: "Rift Bolt",
    symbol: "✦",
    description:
      "Channel a heavy arcane bolt at your selected opponent. Stand still while casting; moving cancels the cast.",
  },
  quell: {
    id: "quell",
    name: "Quell",
    symbol: "⊘",
    description:
      "Off-GCD interrupt against a casting opponent. Cancels their cast and locks out casting for 2 seconds.",
  },
  mend: {
    id: "mend",
    name: "Mend",
    symbol: "✚",
    description:
      "Channel a restorative heal on your friendly target, or yourself when no ally is targeted. Stand still while casting.",
    friendly: true,
  },
};

export const PULSE = SPELLS.pulse;

export function isEngineReady(state: Snapshot | null): state is Snapshot {
  return (
    !!state &&
    state.frames > 10 &&
    ["ready", "failed", "placeholder"].includes(state.character.status)
  );
}

export function spellCooldown(state: Snapshot | null, ability: AbilityId = "pulse"): number {
  if (!state) return 0;
  const spec = state.spells?.[ability];
  if (spec) return spec.cooldown;
  return ability === "pulse" ? state.cooldown : 0;
}

export function spellMaxCooldown(state: Snapshot | null, ability: AbilityId = "pulse"): number {
  if (!state) return 1;
  const spec = state.spells?.[ability];
  if (spec) return Math.max(spec.maxCooldown, spec.cooldown, 0.1);
  return state.pulse.cooldown;
}

export function availability(state: Snapshot | null, ability: AbilityId = "pulse"): string {
  if (!isEngineReady(state)) return "Loading arena";
  if (state.won) return "Round complete";
  const spec = state.spells?.[ability];
  if (spec) {
    if (spec.cooldown > 0) return `Recharging · ${spec.cooldown.toFixed(1)} s`;
    return spec.status;
  }
  if (!state.selected) return SPELLS[ability].friendly ? "Ready" : "Select a target";
  if (state.selectedPlayer && !SPELLS[ability].friendly) return "Invalid target";
  const cd = spellCooldown(state, ability);
  if (cd > 0) return `Recharging · ${cd.toFixed(1)} s`;
  return state.attackStatus;
}

export function canActivate(state: Snapshot | null, ability: AbilityId = "pulse") {
  return availability(state, ability) === "Ready";
}

export function actionTint(
  state: Snapshot | null,
  ability: AbilityId = "pulse",
): "target" | "unavailable" | undefined {
  if (!isEngineReady(state)) return "unavailable";
  const meta = SPELLS[ability];
  const spec = state.spells?.[ability];
  if (!meta.friendly && !state.selected) return undefined;
  if (!meta.friendly && state.selectedPlayer) return "target";
  const status = spec?.status ?? state.attackStatus;
  if (
    state.won ||
    (!meta.friendly &&
      ((state.roster?.find((unit) => unit.actor === state.selectedActor) ?? state.target).health ===
        0 ||
        state.distance > (spec?.range ?? state.range))) ||
    ["Invalid target", "Out of range", "Line of sight blocked"].includes(status)
  )
    return "target";
  return canActivate(state, ability) ? undefined : "unavailable";
}
