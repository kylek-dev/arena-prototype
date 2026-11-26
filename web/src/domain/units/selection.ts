import { handleName } from "../handles";
import type { ActiveCast } from "../casting";

export type Entity = {
  id: string;
  name: string;
  health: number;
  maxHealth: number;
  /** Authoritative primary resource (`0..=maxResource`); absent from older clients. */
  resource?: number;
  maxResource?: number;
  gcdRemaining?: number;
  lockoutRemaining?: number;
  /** Authoritative active timed cast; `null` when idle. */
  cast?: ActiveCast | null;
  /** In a network match: the unit's player is not attached (it stands idle). */
  disconnected?: boolean;
  /** In a network match: the fighter's relayed `Name#number`, which `name`
   * then shows in full; tight spaces show only the name part. */
  handle?: string;
  /** In a network match: the server's admin tag for this fighter's player. */
  admin?: boolean;
};
/** A unit's name for tight spaces (nameplates): a handle without its number. */
export function shortName(unit: Pick<Entity, "name" | "handle">) {
  return unit.handle ? handleName(unit.handle) : unit.name;
}
export function reconcileFocus(focus: string | null, entities: Entity[], reset = false) {
  return !reset && entities.some((e) => e.id === focus) ? focus : null;
}
export function nextTarget(ids: string[], current: string | null): string | null {
  const ordered = [...ids].sort();
  return ordered.length ? ordered[(ordered.indexOf(current ?? "") + 1) % ordered.length] : null;
}
