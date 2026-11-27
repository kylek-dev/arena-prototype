import {
  defaultAppearance,
  parseAppearance,
  parseNetworkAppearances,
  practiceAppearances,
  resolveRosterSpecs,
  type Appearance,
  type Kit,
} from "../domain/appearance";
import { createStore } from "./store";
import { pendingBinding } from "./binding-capture";
import { customLobbyVisible, customizing, prematch, setPanel } from "./session";

export const APPEARANCE_KEY = "arena.appearance.v1";
/** Installed kits from assets/character.json; empty without Quaternius sources. */
export const characterKits = createStore<Kit[]>([]);
/** Relayed per-seat appearances during a network match (local actor order); `null` in practice. */
export const remoteAppearances = createStore<(Appearance | null)[] | null>(null);
export const customizerVisible = customizing;
export const beforeMatch = prematch;
export const preview = createStore({ yaw: 0, zoom: 0 });
export const appearanceSaveError = createStore("");

export function onAppearances(raw: unknown) {
  remoteAppearances.set(parseNetworkAppearances(raw));
}

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
let practiceCache: { kits: Kit[]; looks: Appearance[] } | null = null;
let cache: {
  kits: Kit[];
  look: Appearance;
  remote: (Appearance | null)[] | null;
  revision: number;
  json: string;
} | null = null;
let revision = 0;
export function readAppearance(): unknown[] {
  const kits = characterKits.get();
  const look = appearance.get();
  const remote = remoteAppearances.get();
  if (!cache || cache.kits !== kits || cache.look !== look || cache.remote !== remote) {
    if (!practiceCache || practiceCache.kits !== kits) {
      practiceCache = { kits, looks: practiceAppearances(kits) };
    }
    const specs = resolveRosterSpecs(kits, look, practiceCache.looks, remote);
    if (!specs.length) return [];
    // Keep top-level actor 0 fields alongside `specs` for backwards compatibility.
    cache = {
      kits,
      look,
      remote,
      revision: ++revision,
      json: JSON.stringify({ ...specs[0], specs }),
    };
  }
  return [cache.revision, cache.json];
}
export function readPreview(): number[] {
  const view = preview.get();
  // The launcher page covers the arena; behind it the saved character waits
  // in its close-up, as it did on the old home screen, when there is one.
  const closeUp = customizerVisible.get() || (prematch.get() && characterKits.get().length > 0);
  return [closeUp ? 1 : 0, view.yaw, view.zoom];
}
/** Leaves the launcher page: a fresh round with the saved look. */
export function enterArena(reset: () => void) {
  customizing.set(false);
  prematch.set(false);
  preview.set({ yaw: 0, zoom: 0 });
  reset();
}
/** Closes the panels and overlays a return (to the launcher page or the
 * hub) leaves behind. */
export function closeOverlays() {
  setPanel(null);
  pendingBinding.set(null);
  customizing.set(false);
  customLobbyVisible.set(false);
  preview.set({ yaw: 0, zoom: 0 });
}
/** Returns to the launcher page (outside the game). */
export function returnToLauncher() {
  closeOverlays();
  prematch.set(true);
}
