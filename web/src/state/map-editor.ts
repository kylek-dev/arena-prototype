import {
  commit,
  DEFAULT_MAP_ID,
  historyOf,
  parseMap,
  reconcileSelection,
  redo,
  serializeMap,
  undo,
  type ArenaMap,
  type History,
  type Selection,
} from "../domain/map";
import { createStore } from "./store";

// The editor owns the browser's copy of the map. Each committed change gets a
// new revision; Rust polls `readMap` every frame, validates new revisions and
// answers through `onMapStatus`. Drags only preview in the plan and commit on
// release, so the arena rebuilds once per gesture.

export const mapEditorVisible = createStore(false);
export const mapHistory = createStore<History | null>(null);
export const mapSelection = createStore<Selection>(null);
/** Uncommitted drag preview, drawn in the plan only. */
export const mapPreview = createStore<ArenaMap | null>(null);
export const mapLoad = createStore<{
  state: "idle" | "loading" | "ready" | "failed";
  message?: string;
}>({ state: "idle" });
/** Rust's verdict on a revision: no errors means it is live in the arena. */
export const mapStatus = createStore<{ revision: number; errors: string[] } | null>(null);
/** Text of the file as last loaded or saved, for unsaved-change tracking. */
export const mapSavedText = createStore<string | null>(null);
/** Whether this server can write map files (local dev/preview only). */
export const mapWritable = createStore<boolean | null>(null);
export const mapNotice = createStore<{ tone: "ok" | "error"; text: string } | null>(null);

let sent: { revision: number; json: string } | null = null;
let revision = 0;
export const currentRevision = () => sent?.revision ?? 0;

/** Rust bridge: `[revision, json]`, or nothing before a map has loaded. */
export function readMap(): unknown[] {
  return sent ? [sent.revision, sent.json] : [];
}
export function onMapStatus(raw: unknown) {
  const value = raw as { revision?: unknown; errors?: unknown };
  if (typeof value?.revision !== "number" || !Array.isArray(value.errors)) return;
  mapStatus.set({
    revision: value.revision,
    errors: value.errors.filter((e): e is string => typeof e === "string"),
  });
}

function publish(history: History) {
  mapHistory.set(history);
  mapSelection.set(reconcileSelection(history.present, mapSelection.get()));
  mapPreview.set(null);
  revision += 1;
  sent = { revision, json: JSON.stringify(history.present) };
}

export function commitMap(next: ArenaMap) {
  const history = mapHistory.get();
  if (!history) return;
  const updated = commit(history, next);
  if (updated === history) {
    mapPreview.set(null);
    return;
  }
  publish(updated);
}
export function undoMap() {
  const history = mapHistory.get();
  if (history?.past.length) publish(undo(history));
}
export function redoMap() {
  const history = mapHistory.get();
  if (history?.future.length) publish(redo(history));
}

/** Replaces the map with one read from text (import or reload); undoable. */
export function replaceMap(text: string, saved = false): boolean {
  let map: ArenaMap;
  try {
    map = parseMap(text);
  } catch (e) {
    mapNotice.set({ tone: "error", text: (e as Error).message });
    return false;
  }
  const history = mapHistory.get();
  publish(history ? commit(history, map) : historyOf(map));
  if (saved) mapSavedText.set(serializeMap(map));
  return true;
}

export const mapUrl = (id = DEFAULT_MAP_ID) => new URL(`assets/maps/${id}.json`, document.baseURI);
export const saveUrl = (id = DEFAULT_MAP_ID) =>
  new URL(`__map-editor/maps/${id}.json`, document.baseURI);

/** Loads the map file at startup so the arena uses the file, not a stale build. */
export async function loadMap(id = DEFAULT_MAP_ID) {
  mapLoad.set({ state: "loading" });
  try {
    const response = await fetch(mapUrl(id), { cache: "no-store" });
    if (!response.ok)
      throw new Error(`The map file could not be loaded (HTTP ${response.status}).`);
    const text = await response.text();
    if (!replaceMap(text, true)) throw new Error(mapNotice.get()?.text);
    mapLoad.set({ state: "ready" });
  } catch (e) {
    mapLoad.set({ state: "failed", message: (e as Error).message });
  }
  probeSaving();
}

export async function probeSaving() {
  try {
    const response = await fetch(new URL("__map-editor/status", document.baseURI), {
      cache: "no-store",
    });
    const body = response.ok ? await response.json() : null;
    mapWritable.set(body?.writable === true);
  } catch {
    mapWritable.set(false);
  }
}

/** Writes the current map back to assets/maps through the local dev server. */
export async function saveMap(id = DEFAULT_MAP_ID) {
  const map = mapHistory.get()?.present;
  if (!map) return;
  const text = serializeMap(map);
  try {
    const response = await fetch(saveUrl(id), {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: text,
    });
    const body = await response.json().catch(() => ({}));
    if (!response.ok) throw new Error(body.error ?? `HTTP ${response.status}`);
    mapSavedText.set(text);
    mapNotice.set({ tone: "ok", text: `Saved ${body.saved ?? `assets/maps/${id}.json`}.` });
  } catch (e) {
    mapNotice.set({ tone: "error", text: `Save failed: ${(e as Error).message}` });
  }
}

/** Test and reload hook. */
export function resetMapEditor() {
  sent = null;
  revision = 0;
  mapHistory.set(null);
  mapSelection.set(null);
  mapPreview.set(null);
  mapStatus.set(null);
  mapSavedText.set(null);
  mapWritable.set(null);
  mapNotice.set(null);
  mapLoad.set({ state: "idle" });
  mapEditorVisible.set(false);
}
