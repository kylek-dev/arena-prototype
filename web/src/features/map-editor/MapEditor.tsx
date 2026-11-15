import { useEffect, useRef, type KeyboardEvent } from "react";
import { Button } from "@radix-ui/themes";
import {
  addPillar,
  addRoom,
  DEFAULT_MAP_ID,
  duplicate,
  nudge,
  removeSelected,
  serializeMap,
  type ArenaMap,
  type Selection,
} from "../../domain/map";
import {
  commitMap,
  currentRevision,
  loadMap,
  mapEditorVisible,
  mapHistory,
  mapLoad,
  mapNotice,
  mapPreview,
  mapSavedText,
  mapSelection,
  mapStatus,
  mapWritable,
  redoMap,
  replaceMap,
  saveMap,
  undoMap,
} from "../../state/map-editor";
import { useStore } from "../../state/store";
import { MovableWindow } from "../../ui/MovableWindow";
import { MapInspector } from "./MapInspector";
import { MapPlan } from "./MapPlan";
import "./map-editor.css";

const typing = (target: EventTarget) =>
  target instanceof HTMLElement && !!target.closest("input, select, textarea");

function readText(file: Blob) {
  return new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(reader.error);
    reader.readAsText(file);
  });
}

function download(map: ArenaMap) {
  const url = URL.createObjectURL(new Blob([serializeMap(map)], { type: "application/json" }));
  const link = document.createElement("a");
  link.href = url;
  link.download = `${DEFAULT_MAP_ID}.json`;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 0);
}

function Status({ map }: { map: ArenaMap }) {
  const status = useStore(mapStatus, (s) => s);
  const saved = useStore(mapSavedText, (s) => s);
  const notice = useStore(mapNotice, (s) => s);
  const dirty = saved !== serializeMap(map);
  const current = status?.revision === currentRevision();
  return (
    <div className="map-status stack">
      <p role="status">
        {!current
          ? "Checking map…"
          : status.errors.length
            ? `${status.errors.length} problem${status.errors.length === 1 ? "" : "s"}: the arena keeps the last valid map.`
            : "Valid · live in the arena."}
        {dirty && <span className="map-dirty"> Unsaved changes.</span>}
      </p>
      {current && status.errors.length > 0 && (
        <ul className="map-errors" aria-label="Map problems">
          {status.errors.map((error) => (
            <li key={error}>{error}</li>
          ))}
        </ul>
      )}
      {notice && (
        <p
          className={`map-notice ${notice.tone}`}
          role={notice.tone === "error" ? "alert" : "status"}
        >
          {notice.text}
        </p>
      )}
    </div>
  );
}

function ItemList({ map, selection }: { map: ArenaMap; selection: Selection }) {
  const item = (label: string, value: Selection) => (
    <li key={label}>
      <button
        type="button"
        aria-pressed={JSON.stringify(value) === JSON.stringify(selection)}
        onClick={() => mapSelection.set(value)}
      >
        {label}
      </button>
    </li>
  );
  return (
    <nav aria-label="Map items">
      <ul className="map-items">
        {item("Map & venue", null)}
        {map.pillars.map((_, i) => item(`Pillar ${i + 1}`, { kind: "pillar", index: i }))}
        {map.rooms.map((room, i) =>
          item(`Room ${i + 1} · ${room.team}`, { kind: "room", index: i }),
        )}
      </ul>
    </nav>
  );
}

export function MapEditor() {
  const visible = useStore(mapEditorVisible, (v) => v);
  const history = useStore(mapHistory, (h) => h);
  const preview = useStore(mapPreview, (p) => p);
  const selection = useStore(mapSelection, (s) => s);
  const load = useStore(mapLoad, (l) => l);
  const writable = useStore(mapWritable, (w) => w);
  const saved = useStore(mapSavedText, (s) => s);
  const file = useRef<HTMLInputElement>(null);
  const map = history?.present;
  const dirty = !!map && saved !== serializeMap(map);
  const status = useStore(mapStatus, (s) => s);
  const valid = status?.revision === currentRevision() && status.errors.length === 0;

  useEffect(() => {
    if (!dirty) return;
    const warn = (e: BeforeUnloadEvent) => e.preventDefault();
    addEventListener("beforeunload", warn);
    return () => removeEventListener("beforeunload", warn);
  }, [dirty]);

  if (!visible) return null;
  const close = () => mapEditorVisible.set(false);

  const keys = (e: KeyboardEvent) => {
    if (!map) return;
    const mod = e.ctrlKey || e.metaKey;
    const handled = () => {
      e.preventDefault();
      e.stopPropagation();
    };
    if (e.key === "Escape") {
      handled();
      if (selection) mapSelection.set(null);
      else close();
      return;
    }
    if (typing(e.target)) return;
    if (mod && e.key.toLowerCase() === "z") {
      handled();
      if (e.shiftKey) redoMap();
      else undoMap();
    } else if (mod && e.key.toLowerCase() === "y") {
      handled();
      redoMap();
    } else if (mod && e.key.toLowerCase() === "d" && selection?.kind === "pillar") {
      handled();
      const next = duplicate(map, selection);
      commitMap(next.map);
      mapSelection.set(next.selection);
    } else if ((e.key === "Delete" || e.key === "Backspace") && selection) {
      handled();
      commitMap(removeSelected(map, selection));
    } else if (e.key.startsWith("Arrow") && selection) {
      handled();
      const step = e.shiftKey ? 2 : 0.5;
      const dx = e.key === "ArrowLeft" ? -step : e.key === "ArrowRight" ? step : 0;
      const dz = e.key === "ArrowUp" ? -step : e.key === "ArrowDown" ? step : 0;
      commitMap(nudge(map, selection, dx, dz));
    }
  };

  return (
    <MovableWindow
      className="map-editor-window"
      title="Map editor"
      close={close}
      anchor={{ x: 0.02, y: 0.03 }}
    >
      <div className="map-editor stack" onKeyDown={keys}>
        {!map ? (
          <div className="stack">
            <p role="status">{load.state === "failed" ? load.message : "Loading map…"}</p>
            {load.state === "failed" && (
              <Button variant="surface" onClick={() => loadMap()}>
                Try again
              </Button>
            )}
          </div>
        ) : (
          <>
            <div className="map-toolbar" role="toolbar" aria-label="Map tools">
              <Button
                size="1"
                variant="surface"
                color="gray"
                disabled={!history?.past.length}
                onClick={undoMap}
              >
                Undo
              </Button>
              <Button
                size="1"
                variant="surface"
                color="gray"
                disabled={!history?.future.length}
                onClick={redoMap}
              >
                Redo
              </Button>
              <Button
                size="1"
                variant="surface"
                onClick={() => {
                  const next = addPillar(map);
                  commitMap(next);
                  mapSelection.set({ kind: "pillar", index: next.pillars.length - 1 });
                }}
              >
                Add pillar
              </Button>
              <Button
                size="1"
                variant="surface"
                onClick={() => {
                  const next = addRoom(map, "friendly");
                  commitMap(next);
                  mapSelection.set({ kind: "room", index: next.rooms.length - 1 });
                }}
              >
                Add friendly room
              </Button>
              <Button
                size="1"
                variant="surface"
                onClick={() => {
                  const next = addRoom(map, "opposing");
                  commitMap(next);
                  mapSelection.set({ kind: "room", index: next.rooms.length - 1 });
                }}
              >
                Add opposing room
              </Button>
              <span className="map-toolbar-gap" />
              {writable && (
                <Button
                  size="1"
                  disabled={!dirty || !valid}
                  title={!valid ? "Fix the listed problems before saving." : undefined}
                  onClick={() => saveMap()}
                >
                  Save to assets/maps
                </Button>
              )}
              <Button size="1" variant="surface" color="gray" onClick={() => download(map)}>
                Download
              </Button>
              <Button size="1" variant="surface" color="gray" onClick={() => file.current?.click()}>
                Import…
              </Button>
              <Button size="1" variant="ghost" color="gray" onClick={() => loadMap()}>
                Reload file
              </Button>
              <input
                ref={file}
                type="file"
                accept=".json,application/json"
                hidden
                aria-label="Import map file"
                onChange={async (e) => {
                  const chosen = e.target.files?.[0];
                  e.target.value = "";
                  if (chosen && replaceMap(await readText(chosen)))
                    mapNotice.set({ tone: "ok", text: `Imported ${chosen.name}.` });
                }}
              />
            </div>
            <Status map={map} />
            <div className="map-workspace">
              <div className="map-canvas stack">
                <MapPlan map={map} preview={preview} selection={selection} />
                <p className="map-hint">
                  Drag pillars, rooms and spawns; corner and back handles resize. Alt: 0.1 m snap.
                  Arrows nudge (Shift ×4), Delete removes, Ctrl+Z / Ctrl+Shift+Z undo and redo.
                  Edits go live in the arena; the round keeps going unless a unit ends up inside a
                  wall.
                  {writable === false &&
                    " Saving needs the local dev server; use Download instead."}
                </p>
              </div>
              <div className="map-side stack">
                <ItemList map={map} selection={selection} />
                <MapInspector map={map} selection={selection} />
              </div>
            </div>
          </>
        )}
      </div>
    </MovableWindow>
  );
}
