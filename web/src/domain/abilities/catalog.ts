import type { Snapshot } from "../../bridge/snapshot";
import type { AbilityId } from "../action-bars/model";
// Prepared by scripts/prepare_icons.py; Vite fingerprints the URLs.
import pulseIcon1x from "../../../../assets/icons/spells/pulse-128.webp";
import pulseIcon2x from "../../../../assets/icons/spells/pulse-256.webp";
import riftBoltIcon1x from "../../../../assets/icons/spells/rift-bolt-128.webp";
import riftBoltIcon2x from "../../../../assets/icons/spells/rift-bolt-256.webp";
import quellIcon1x from "../../../../assets/icons/spells/quell-128.webp";
import quellIcon2x from "../../../../assets/icons/spells/quell-256.webp";
import mendIcon1x from "../../../../assets/icons/spells/mend-128.webp";
import mendIcon2x from "../../../../assets/icons/spells/mend-256.webp";

/** A painted icon at 1x and 2x for the 64 px slot. Spells without one fall back to `symbol`. */
export type SpellIconImage = { x1: string; x2: string };

export type SpellMeta = {
  id: AbilityId;
  name: string;
  symbol: string;
  icon?: SpellIconImage;
  description: string;
  friendly?: boolean;
};

export const SPELLS: Record<AbilityId, SpellMeta> = {
  pulse: {
    id: "pulse",
    name: "Pulse",
    symbol: "ϟ",
    icon: { x1: pulseIcon1x, x2: pulseIcon2x },
    description:
      "Release a focused burst at your selected opponent. Instant, usable while moving, requires range and line of sight.",
  },
  rift_bolt: {
    id: "rift_bolt",
    name: "Rift Bolt",
    symbol: "✦",
    icon: { x1: riftBoltIcon1x, x2: riftBoltIcon2x },
    description:
      "Channel a heavy arcane bolt at your selected opponent. Stand still while casting; moving cancels the cast.",
  },
  quell: {
    id: "quell",
    name: "Quell",
    symbol: "⊘",
    icon: { x1: quellIcon1x, x2: quellIcon2x },
    description:
      "Off-GCD interrupt against a casting opponent. Cancels their cast and locks out casting for 2 seconds.",
  },
  mend: {
    id: "mend",
    name: "Mend",
    symbol: "✚",
    icon: { x1: mendIcon1x, x2: mendIcon2x },
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

/** How far behind the server the HUD's timers may be (snapshot age and
 * interpolation), so presses near the window's edge still reach it. */
export const CAST_BUFFER_SLACK_MS = 500;

/** Whether a press the HUD cannot cast now should still go to the server for
 * its cast buffer (ADR 0058): the buffer is on, and the player's cast, the
 * GCD or the spell's own cooldown is running and looks close enough to its
 * end. The server judges the window against its own, newer timers and
 * validates the spell when it fires. */
export function canBuffer(state: Snapshot | null, ability: AbilityId, bufferMs: number) {
  if (bufferMs <= 0 || !isEngineReady(state) || state.won) return false;
  const tps = state.tickRate ?? 30;
  const waitSeconds = Math.max(
    (state.player.cast?.remainingTicks ?? 0) / tps,
    (state.player.gcdRemaining ?? 0) / tps,
    spellCooldown(state, ability),
  );
  return waitSeconds > 0 && waitSeconds * 1000 <= bufferMs + CAST_BUFFER_SLACK_MS;
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
