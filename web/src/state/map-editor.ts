import {
  commit,
  DEFAULT_MAP_ID,
  historyOf,
  MAP_ID,
  parseMap,
  reconcileSelection,
  redo,
  serializeMap,
  undo,
  type ArenaMap,
  type History,
  type PropInfo,
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

/**
 * Whether this player may open the map editor: admins (the door checks every
 * admin request again), or anyone on the local authoring server (`npm run dev`
 * or preview, whose `__map-editor` endpoints a deployed host never serves).
 */
export const mapEditorAllowed = (admin: boolean, writable = mapWritable.get()) =>
  admin || writable === true;
export const mapNotice = createStore<{ tone: "ok" | "error"; text: string } | null>(null);
/** File id (assets/maps/<id>.json) of the map being edited and played. */
export const mapId = createStore(DEFAULT_MAP_ID);
/** Maps available to open; the current one is always included. */
export const mapList = createStore<{ id: string; name: string }[]>([]);
/** Placeable props (assets/props/catalog.json); empty until loaded or if missing. */
export const propCatalog = createStore<PropInfo[]>([]);

/** The map the arena starts on next time (per device). */
const LAST_MAP_KEY = "arena.map.v1";
function rememberMap(id: string) {
  try {
    localStorage.setItem(LAST_MAP_KEY, id);
  } catch {
    // Storage may be unavailable (private windows); the default map is used.
  }
}
export function rememberedMap(): string {
  try {
    const id = localStorage.getItem(LAST_MAP_KEY);
    return id && MAP_ID.test(id) ? id : DEFAULT_MAP_ID;
  } catch {
    return DEFAULT_MAP_ID;
  }
}

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

/**
 * Replaces the map with one read from text (import or reload). Undoable,
 * unless `fresh` starts a new history (switching to another map file).
 */
export function replaceMap(text: string, saved = false, fresh = false): boolean {
  let map: ArenaMap;
  try {
    map = parseMap(text);
  } catch (e) {
    mapNotice.set({ tone: "error", text: (e as Error).message });
    return false;
  }
  startMap(map, saved, fresh);
  return true;
}
function startMap(map: ArenaMap, saved: boolean, fresh: boolean) {
  const history = fresh ? null : mapHistory.get();
  if (fresh) mapSelection.set(null);
  publish(history ? commit(history, map) : historyOf(map));
  if (saved) mapSavedText.set(serializeMap(map));
  else if (fresh) mapSavedText.set(null);
}

export const mapUrl = (id = DEFAULT_MAP_ID) => new URL(`assets/maps/${id}.json`, document.baseURI);
export const saveUrl = (id = DEFAULT_MAP_ID) =>
  new URL(`__map-editor/maps/${id}.json`, document.baseURI);

/**
 * Loads a map file and makes it live, so the arena uses the file, not a stale
 * build. Another id than the current one starts a fresh history.
 */
export async function loadMap(id = mapId.get()) {
  mapLoad.set({ state: "loading" });
  try {
    const response = await fetch(mapUrl(id), { cache: "no-store" });
    if (!response.ok)
      throw new Error(`The map file could not be loaded (HTTP ${response.status}).`);
    const text = await response.text();
    const other = id !== mapId.get() || !mapHistory.get();
    if (!replaceMap(text, true, other)) throw new Error(mapNotice.get()?.text);
    mapId.set(id);
    mapSource.set("local");
    serverSavedText.set(null);
    rememberMap(id);
    mapLoad.set({ state: "ready" });
  } catch (e) {
    mapLoad.set({ state: "failed", message: (e as Error).message });
    // With a map already open, the failure would otherwise go unseen.
    if (mapHistory.get()) mapNotice.set({ tone: "error", text: (e as Error).message });
  }
  probeSaving();
}

/** Startup: the map last opened on this device, falling back to the default. */
export async function loadStartupMap() {
  const id = rememberedMap();
  await loadMap(id);
  if (mapLoad.get().state === "failed" && id !== DEFAULT_MAP_ID) {
    mapId.set(DEFAULT_MAP_ID);
    await loadMap(DEFAULT_MAP_ID);
  }
}

/** Starts a new, unsaved map file `id` and makes it live in the arena. */
export function createMap(id: string, map: ArenaMap): string | null {
  if (!MAP_ID.test(id)) return "Map ids use lowercase letters, digits and dashes.";
  if (mapList.get().some((m) => m.id === id)) return `A map called "${id}" already exists.`;
  startMap(map, false, true);
  mapId.set(id);
  mapSource.set("local");
  serverSavedText.set(null);
  mapList.set([...mapList.get(), { id, name: map.name }]);
  mapLoad.set({ state: "ready" });
  mapNotice.set({ tone: "ok", text: `New map ${id}: save it to keep it.` });
  return null;
}

/** The `{maps: [{id, name}]}` of a map listing, keeping well-formed entries;
 * `null` when the body is not a listing. */
function parseMapList(body: unknown) {
  const maps = (body as { maps?: unknown } | null)?.maps;
  if (!Array.isArray(maps)) return null;
  return maps.filter(
    (m): m is { id: string; name: string } =>
      typeof (m as { id?: unknown })?.id === "string" &&
      MAP_ID.test((m as { id: string }).id) &&
      typeof (m as { name?: unknown }).name === "string",
  );
}

/** Map files to open: the dev server lists assets/maps; builds ship an index. */
export async function listMaps() {
  const read = async (url: URL) => {
    const response = await fetch(url, { cache: "no-store" });
    if (!response.ok) throw new Error();
    const maps = parseMapList(await response.json());
    if (!maps) throw new Error();
    return maps;
  };
  let maps: { id: string; name: string }[] = [];
  for (const url of [
    new URL("__map-editor/maps", document.baseURI),
    new URL("assets/maps/index.json", document.baseURI),
  ]) {
    try {
      maps = await read(url);
      break;
    } catch {
      // Try the next source.
    }
  }
  const current = mapId.get();
  if (mapSource.get() === "local" && !maps.some((m) => m.id === current))
    maps.push({ id: current, name: mapHistory.get()?.present.name ?? current });
  mapList.set(maps.sort((a, b) => a.id.localeCompare(b.id)));
}

export async function loadPropCatalog() {
  if (propCatalog.get().length) return;
  try {
    const response = await fetch(new URL("assets/props/catalog.json", document.baseURI));
    const body = response.ok ? await response.json() : null;
    if (Array.isArray(body?.props)) propCatalog.set(body.props as PropInfo[]);
  } catch {
    // Props then draw as placeholders and cannot be added.
  }
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
export async function saveMap(id = mapId.get()) {
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
    rememberMap(id);
    mapNotice.set({ tone: "ok", text: `Saved ${body.saved ?? `assets/maps/${id}.json`}.` });
  } catch (e) {
    mapNotice.set({ tone: "error", text: `Save failed: ${(e as Error).message}` });
  }
}

// Admin map storage on the front door (social-identity step 5): admins can
// keep maps on the server. The door checks the role on every request; the
// page only hides the buttons from everyone else.

/** Where the open map came from: a map file, or the server's storage. */
export const mapSource = createStore<"local" | "server">("local");
/** The map as last opened from or saved on the server; dirty tracking for
 * the server copy, apart from `mapSavedText` (the map file). */
export const serverSavedText = createStore<string | null>(null);
/** Maps saved on the server (`GET /api/admin/maps`); `null` until listed. */
export const serverMaps = createStore<{ id: string; name: string }[] | null>(null);
/** Why the server refused the last save (`invalid_map` problems). */
export const serverProblems = createStore<string[] | null>(null);
const adminUrl = (path: string) => new URL(`/api/admin/maps${path}`, window.location.origin);
const ADMIN_ERRORS: Record<number, string> = {
  401: "Log in as an admin to use server maps.",
  404: "The server has no such map, or no map storage.",
  413: "The map is too big for the server (256 KiB at most).",
  429: "Too many requests. Wait a moment and try again.",
};
/** Words for a refused admin request; a 403 is either the role or, as plain
 * text, an origin the door does not allow. */
const adminError = (status: number, body?: unknown) => {
  if (status === 403)
    return (body as { error?: unknown } | null)?.error === "not_admin"
      ? "Only admins can use server maps."
      : "This page is not allowed to use server maps. Open the game from its own address.";
  return ADMIN_ERRORS[status] ?? `The server could not be used (HTTP ${status}).`;
};
const problemList = (raw: unknown) =>
  Array.isArray(raw) ? raw.filter((p): p is string => typeof p === "string").slice(0, 50) : [];

/** Lists the maps saved on the server. */
export async function listServerMaps() {
  try {
    const response = await fetch(adminUrl(""), { cache: "no-store", credentials: "same-origin" });
    const body = await response.json().catch(() => null);
    if (!response.ok) throw new Error(adminError(response.status, body));
    const maps = parseMapList(body);
    if (!maps) throw new Error("The server's map list was not understood.");
    serverMaps.set(maps.sort((a, b) => a.id.localeCompare(b.id)));
  } catch (e) {
    serverMaps.set([]);
    mapNotice.set({ tone: "error", text: (e as Error).message });
  }
}

/** Opens a map saved on the server, live in the arena, as map `id`. */
export async function openServerMap(id: string) {
  if (!MAP_ID.test(id)) return;
  try {
    const response = await fetch(adminUrl(`/${id}.json`), {
      cache: "no-store",
      credentials: "same-origin",
    });
    if (!response.ok)
      throw new Error(adminError(response.status, await response.json().catch(() => null)));
    const text = await response.text();
    // Not saved as a map file: the local save stays available.
    if (!replaceMap(text, false, true)) throw new Error(mapNotice.get()?.text);
    mapSavedText.set(null);
    serverSavedText.set(serializeMap(mapHistory.get()!.present));
    mapId.set(id);
    mapSource.set("server");
    serverProblems.set(null);
    mapLoad.set({ state: "ready" });
    mapNotice.set({ tone: "ok", text: `Opened ${id} from the server.` });
  } catch (e) {
    mapNotice.set({ tone: "error", text: `Open failed: ${(e as Error).message}` });
  }
}

/** Saves the current map on the server as `id`, where the door validates it
 * as the editor and the build do. */
export async function saveServerMap(id = mapId.get()) {
  const map = mapHistory.get()?.present;
  if (!map || !MAP_ID.test(id)) return;
  const text = serializeMap(map);
  serverProblems.set(null);
  try {
    const response = await fetch(adminUrl(`/${id}.json`), {
      method: "PUT",
      credentials: "same-origin",
      headers: { "Content-Type": "application/json" },
      body: text,
    });
    const body = await response.json().catch(() => ({}));
    if (response.status === 422 && body?.error === "invalid_map") {
      const problems = problemList(body.problems);
      if (!problems.length) throw new Error("the server could not read the map.");
      serverProblems.set(problems);
      throw new Error(
        `the server found ${problems.length} problem${problems.length === 1 ? "" : "s"} (listed below).`,
      );
    }
    if (response.status === 422 && body?.error === "invalid_id")
      throw new Error("map ids use lowercase letters, digits and dashes.");
    if (!response.ok) throw new Error(adminError(response.status, body));
    serverSavedText.set(text);
    mapSource.set("server");
    const saved = { id, name: typeof body?.name === "string" ? body.name : map.name };
    serverMaps.set(
      [...(serverMaps.get() ?? []).filter((m) => m.id !== id), saved].sort((a, b) =>
        a.id.localeCompare(b.id),
      ),
    );
    mapNotice.set({ tone: "ok", text: `Saved ${id} on the server.` });
  } catch (e) {
    mapNotice.set({ tone: "error", text: `Server save failed: ${(e as Error).message}` });
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
  mapId.set(DEFAULT_MAP_ID);
  mapList.set([]);
  propCatalog.set([]);
  serverMaps.set(null);
  serverProblems.set(null);
  serverSavedText.set(null);
  mapSource.set("local");
}
