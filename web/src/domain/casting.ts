// Pure casting & resource presentation helpers (ADR 0048).
// No DOM, React, or engine dependencies.

export type SpellKey = "pulse" | "rift_bolt" | "quell" | "mend";

export type ActiveCast = {
  spell: SpellKey;
  remainingTicks: number;
  totalTicks: number;
};

const SPELL_KEYS = new Set<SpellKey>(["pulse", "rift_bolt", "quell", "mend"]);
const TIMED_SPELL_KEYS = new Set<SpellKey>(["rift_bolt", "mend"]);

const SPELL_LABELS: Record<SpellKey, string> = {
  pulse: "Pulse",
  rift_bolt: "Rift Bolt",
  quell: "Quell",
  mend: "Mend",
};

export function isValidSpellKey(value: unknown): value is SpellKey {
  return typeof value === "string" && SPELL_KEYS.has(value as SpellKey);
}

export function isValidActiveCast(value: unknown): value is ActiveCast {
  if (!value || typeof value !== "object") return false;
  const c = value as ActiveCast;
  return (
    isValidSpellKey(c.spell) &&
    TIMED_SPELL_KEYS.has(c.spell) &&
    Number.isSafeInteger(c.totalTicks) &&
    c.totalTicks >= 1 &&
    Number.isSafeInteger(c.remainingTicks) &&
    c.remainingTicks >= 1 &&
    c.remainingTicks <= c.totalTicks
  );
}

export function spellLabel(spell: SpellKey): string {
  return SPELL_LABELS[spell];
}

/** Completion ratio in `[0, 1]` for a cast bar (`0` at start, approaching `1` at completion). */
export function castProgress(cast: ActiveCast): number {
  if (cast.totalTicks <= 0) return 0;
  const elapsed = cast.totalTicks - cast.remainingTicks;
  return Math.min(1, Math.max(0, elapsed / cast.totalTicks));
}

/** Resource fill percentage in `[0, 100]` for a unit frame meter. */
export function resourcePercent(resource: number, maxResource: number): number {
  if (!Number.isFinite(resource) || !Number.isFinite(maxResource) || maxResource <= 0) return 0;
  return Math.min(100, Math.max(0, (resource / maxResource) * 100));
}
