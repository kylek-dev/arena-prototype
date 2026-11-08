import {
  defaultWeapons,
  parseWeapons,
  weaponValues,
  type WeaponPreferences,
} from "../domain/weapons";
import { createStore } from "./store";

export const WEAPONS_KEY = "arena.weapons.v1";
export function loadWeapons(storage: Pick<Storage, "getItem">): WeaponPreferences {
  try {
    return (
      parseWeapons(JSON.parse(storage.getItem(WEAPONS_KEY) ?? "null")) ?? { ...defaultWeapons }
    );
  } catch {
    return { ...defaultWeapons };
  }
}
function initialWeapons() {
  try {
    return loadWeapons(localStorage);
  } catch {
    return { ...defaultWeapons };
  }
}
export const weapons = createStore(initialWeapons());
export const weaponsSaveError = createStore("");
export const readWeapons = () => weaponValues(weapons.get());
export function updateWeapons(patch: Partial<Pick<WeaponPreferences, "weapon" | "color">>) {
  const next = parseWeapons({ ...weapons.get(), ...patch });
  if (!next) return;
  weapons.set(next);
  try {
    localStorage.setItem(WEAPONS_KEY, JSON.stringify(next));
    weaponsSaveError.set("");
  } catch {
    weaponsSaveError.set("Weapon appearance applies now but could not be saved on this device.");
  }
}
