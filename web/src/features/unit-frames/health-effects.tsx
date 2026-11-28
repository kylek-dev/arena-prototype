import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { Entity } from "../../domain/units/selection";

// Health reactions shared by the unit frames and the nameplates.

/** At or below this share of its health a living unit's frame pulses ember. */
export const LOW_HEALTH = 0.25;
/** How long a heal's growth and wave play (ms); see unit-frames.css. */
const HEAL_MS = 700;

/** A recent loss drawn behind the health, from `to` up to `from` (0–1). */
type Trail = { key: number; from: number; to: number };

/**
 * The frame's reactions to its unit's health changing: a heal grows the
 * bar with a wave, a loss leaves a trail that drains away. A different
 * unit in the frame (a new target) starts without either.
 */
export function useHealthChanges(unit: Entity | null) {
  const previous = useRef<{ id: string; health: number } | null>(null);
  const counter = useRef(0);
  const [trail, setTrail] = useState<Trail | null>(null);
  const [heal, setHeal] = useState<number | null>(null);
  const id = unit?.id ?? null;
  const health = unit?.health ?? 0;
  const max = unit?.maxHealth ?? 1;
  useLayoutEffect(() => {
    const before = previous.current;
    previous.current = id === null ? null : { id, health };
    if (!before || before.id !== id) {
      setTrail(null);
      setHeal(null);
      return;
    }
    if (health < before.health) {
      const key = ++counter.current;
      const now = health / max;
      // Hits in quick succession extend the trail still draining.
      setTrail((t) => ({ key, from: Math.max(t?.from ?? 0, before.health / max), to: now }));
      setHeal(null);
    } else if (health > before.health) {
      setHeal(++counter.current);
      setTrail(null);
    }
  }, [id, health, max]);
  useEffect(() => {
    if (heal === null) return;
    const timer = setTimeout(() => setHeal(null), HEAL_MS);
    return () => clearTimeout(timer);
  }, [heal]);
  return { trail, heal, endTrail: () => setTrail(null) };
}

/** The spark on a fill's leading edge: not on an empty or full bar. */
export function Spark({ percent }: { percent: number }) {
  return percent > 0 && percent < 100 ? <span className="bar-spark" aria-hidden="true" /> : null;
}

/** A living, connected unit at or below `LOW_HEALTH` of its health. */
export function isLowHealth(unit: Entity | null) {
  return (
    !!unit && !unit.disconnected && unit.health > 0 && unit.health / unit.maxHealth <= LOW_HEALTH
  );
}
