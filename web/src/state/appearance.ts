import {
  defaultAppearance,
  parseAppearance,
  renderSpec,
  selectKit,
  type Appearance,
  type Kit,
} from "../domain/appearance";
import { createStore } from "./store";
import { customizing, prematch } from "./session";

export const APPEARANCE_KEY = "arena.appearance.v1";
/** Installed kits from assets/character.json; empty without Quaternius sources. */
export const characterKits = createStore<Kit[]>([]);
export const customizerVisible = customizing;
export const beforeMatch = prematch;
export const preview = createStore({ yaw: 0, zoom: 0 });
export const appearanceSaveError = createStore("");

export function loadAppearance(storage: Pick<Storage, "getItem">): Appearance {
  try {
    return (
      parseAppearance(JSON.parse(storage.getItem(APPEARANCE_KEY) ?? "null")) ?? {
        ...defaultAppearance,
      }
    );
  } catch {
    return { ...defaultAppearance };
  }
}
function initial() {
  try {
    return loadAppearance(localStorage);
  } catch {
    return { ...defaultAppearance };
  }
}
export const appearance = createStore(initial());

export function updateAppearance(next: Appearance) {
  appearance.set(next);
  try {
    localStorage.setItem(APPEARANCE_KEY, JSON.stringify(next));
    appearanceSaveError.set("");
  } catch {
    appearanceSaveError.set("Your look applies now but could not be saved on this device.");
  }
}

// The engine polls this every frame; the JSON is rebuilt only on change.
let cache: { kits: Kit[]; look: Appearance; revision: number; json: string } | null = null;
let revision = 0;
export function readAppearance(): unknown[] {
  const kits = characterKits.get();
  const look = appearance.get();
  if (!cache || cache.kits !== kits || cache.look !== look) {
    const kit = selectKit(kits, look);
    if (!kit) return [];
    cache = { kits, look, revision: ++revision, json: JSON.stringify(renderSpec(kit, look)) };
  }
  return [cache.revision, cache.json];
}
export function readPreview(): number[] {
  const view = preview.get();
  return [customizerVisible.get() || prematch.get() ? 1 : 0, view.yaw, view.zoom];
}
/** Leaves the start screen: a fresh round with the saved look. */
export function enterArena(reset: () => void) {
  customizing.set(false);
  prematch.set(false);
  preview.set({ yaw: 0, zoom: 0 });
  reset();
}
