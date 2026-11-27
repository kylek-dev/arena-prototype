import type { UnitTag } from "../domain/units/tags";
import { createStore } from "./store";

/** The right-click menu on a unit (frames and friendly nameplates): the
 * slot it was opened on and the unit there at that moment (the menu closes
 * if the slot shows someone else), where it opened, and the layout frame it
 * was opened from (for "Edit layout"), if any. */
export type UnitMenuRequest = {
  tag: UnitTag;
  unitId: string;
  x: number;
  y: number;
  frame: string | null;
};
export const unitMenu = createStore<UnitMenuRequest | null>(null);

export function openUnitMenu(request: UnitMenuRequest) {
  unitMenu.set(request);
}
export function closeUnitMenu() {
  unitMenu.set(null);
}
