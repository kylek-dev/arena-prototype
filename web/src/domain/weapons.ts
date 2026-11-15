export const weaponKinds = ["glaives", "staff"] as const;
export const weaponColors = ["purple", "yellow", "red", "blue", "green", "cyan"] as const;
export interface WeaponPreferences {
  version: 1;
  weapon: (typeof weaponKinds)[number];
  color: (typeof weaponColors)[number];
}
export const defaultWeapons: WeaponPreferences = {
  version: 1,
  weapon: "glaives",
  color: "purple",
};
export function parseWeapons(value: unknown): WeaponPreferences | null {
  if (!value || typeof value !== "object") return null;
  const v = value as Record<string, unknown>;
  if (
    v.version !== 1 ||
    !weaponKinds.some((kind) => kind === v.weapon) ||
    !weaponColors.some((color) => color === v.color)
  )
    return null;
  return { version: 1, weapon: v.weapon, color: v.color } as WeaponPreferences;
}
// Presentation only; not part of the combat input ABI.
export function weaponValues(value: WeaponPreferences): number[] {
  return [weaponKinds.indexOf(value.weapon), weaponColors.indexOf(value.color)];
}
