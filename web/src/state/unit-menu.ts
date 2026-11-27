import type { UnitTag } from "../domain/units/tags";
import { createStore } from "./store";

/** The right-click menu on a unit (frames and friendly nameplates): the
 * slot it was opened on and the unit there at that moment (the menu closes
 * if the slot shows someone else), where it opened, and the layout frame it
 * was opened from (for "Edit layout"), if any. A party member who is not in
 * this match has no unit: their menu is by handle (and closes when they
 * leave the party). */
export type UnitMenuRequest =
  | {
      kind?: "unit";
      tag: UnitTag;
      unitId: string;
      x: number;
      y: number;
      frame: string | null;
    }
  | { kind: "party"; handle: string; x: number; y: number; frame: "team" };
export const unitMenu = createStore<UnitMenuRequest | null>(null);

export function openUnitMenu(request: UnitMenuRequest) {
  unitMenu.set(request);
}
export function closeUnitMenu() {
  unitMenu.set(null);
}
