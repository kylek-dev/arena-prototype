import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { Button } from "@radix-ui/themes";
import {
  addPillar,
  addProp,
  addRoom,
  duplicate,
  GROUNDS,
  mapIdFor,
  newMap,
  nudge,
  removeSelected,
  rotateProp,
  serializeMap,
  type ArenaMap,
  type Ground,
  type Selection,
} from "../../domain/map";
import {
  commitMap,
  createMap,
  currentRevision,
  listMaps,
  loadMap,
  loadPropCatalog,
  mapEditorVisible,
  mapId,
  mapList,
  mapHistory,
  mapLoad,
  mapNotice,
  mapPreview,
  mapSavedText,
  mapSelection,
  mapStatus,
  mapWritable,
  propCatalog,
  redoMap,
  replaceMap,
  saveMap,
  saveServerMap,
  serverMaps,
  serverProblems,
  listServerMaps,
  openServerMap,
  mapSource,
  serverSavedText,
  undoMap,
} from "../../state/map-editor";
import { accountService } from "../../state/account";
import { LOADING_ACCOUNT } from "../../runtime/account";
import { createStore, useStore } from "../../state/store";
import { MovableWindow } from "../../ui/MovableWindow";
import { MapInspector, PropOptions } from "./MapInspector";
import { MapPlan } from "./MapPlan";
import "./map-editor.css";

const NO_ACCOUNT = createStore(LOADING_ACCOUNT);
/** Whether the door says this player is an admin (it checks every request). */
function useAdmin() {
  const account = useStore(accountService, (a) => a);
  return useStore(account?.state ?? NO_ACCOUNT, (s) => !!s.me?.admin);
}

/** Asks before an open replaces unsaved changes. */
function DiscardPrompt({
  label,
  onOpen,
  onKeep,
}: {
  label: string;
  onOpen: () => void;
  onKeep: () => void;
}) {
  return (
    <span role="alert" className="map-notice error">
      Discard unsaved changes?{" "}
      <Button size="1" className="action-button" color="red" variant="soft" onClick={onOpen}>
        {label}
      </Button>{" "}
      <Button size="1" className="action-button" variant="surface" color="gray" onClick={onKeep}>
        Keep editing
      </Button>
    </span>
  );
}

/** Admins only: open a map saved on the server, or save this one there
 * (asking before replacing a server map this one did not come from). */
function ServerMaps({ dirty, valid, id }: { dirty: boolean; valid: boolean; id: string }) {
  const maps = useStore(serverMaps, (m) => m);
  const source = useStore(mapSource, (s) => s);
  const [pending, setPending] = useState<string | null>(null);
  const [replacing, setReplacing] = useState(false);
  const open = (next: string) => {
    setPending(null);
    void openServerMap(next);
  };
  const taken = source !== "server" && !!maps?.some((m) => m.id === id);
  return (
    <>
      {replacing ? (
        <span role="alert" className="map-notice error">
          The server already has a map called {id}.{" "}
          <Button
            size="1"
            className="action-button"
            color="red"
            variant="soft"
            onClick={() => {
              setReplacing(false);
              void saveServerMap();
            }}
          >
            Replace it
          </Button>{" "}
          <Button
            size="1"
            className="action-button"
            variant="surface"
            color="gray"
            onClick={() => setReplacing(false)}
          >
            Keep it
          </Button>
        </span>
      ) : (
        <Button
          size="1"
          disabled={!valid}
          title={!valid ? "Fix the listed problems before saving." : undefined}
          onClick={() => (taken ? setReplacing(true) : void saveServerMap())}
        >
          Save {id} to server
        </Button>
      )}
      <select
        aria-label="Open from server"
        value=""
        disabled={!maps?.length}
        onChange={(e) => {
          const next = e.target.value;
          if (next) dirty ? setPending(next) : open(next);
        }}
      >
        <option value="">{maps?.length ? "Open from server…" : "No server maps"}</option>
        {maps?.map((m) => (
          <option key={m.id} value={m.id}>
            {m.name} ({m.id})
          </option>
        ))}
      </select>
      {pending && (
        <DiscardPrompt
          label={`Open ${pending} from server`}
          onOpen={() => open(pending)}
          onKeep={() => setPending(null)}
        />
      )}
    </>
  );
}

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

function download(map: ArenaMap, id: string) {
  const url = URL.createObjectURL(new Blob([serializeMap(map)], { type: "application/json" }));
  const link = document.createElement("a");
  link.href = url;
  link.download = `${id}.json`;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 0);
}

function Status({ map }: { map: ArenaMap }) {
  const status = useStore(mapStatus, (s) => s);
  const saved = useStore(mapSavedText, (s) => s);
  const notice = useStore(mapNotice, (s) => s);
  const problems = useStore(serverProblems, (p) => p);
  const serverSaved = useStore(serverSavedText, (s) => s);
  const text = serializeMap(map);
  const dirty = saved !== text && serverSaved !== text;
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
      {problems && problems.length > 0 && (
        <ul className="map-errors" aria-label="Server map problems">
          {problems.map((problem) => (
            <li key={problem}>{problem}</li>
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
        {map.props.map((prop, i) =>
          item(`Prop ${i + 1} · ${prop.model.replaceAll("_", " ")}`, { kind: "prop", index: i }),
        )}
      </ul>
    </nav>
  );
}

/** Name, file id and ground for a new map; replaces the edited map once created. */
function NewMapForm({ dirty, close }: { dirty: boolean; close: () => void }) {
  const [name, setName] = useState("");
  const [id, setId] = useState("");
  const [ground, setGround] = useState<Ground>("stone");
  const [error, setError] = useState<string | null>(null);
  const fileId = id || mapIdFor(name || "New arena");
  const create = () => {
    const problem = createMap(fileId, newMap(name.trim() || "New arena", ground));
    if (problem) setError(problem);
    else close();
  };
  return (
    <section className="map-new" aria-label="New map">
      <label className="map-field map-field-wide">
        <span>Name</span>
        <input
          value={name}
          placeholder="New arena"
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && create()}
        />
      </label>
      <label className="map-field map-field-wide">
        <span>File id</span>
        <input
          value={id}
          placeholder={fileId}
          onChange={(e) => setId(e.target.value.toLowerCase())}
        />
      </label>
      <label className="map-field map-field-wide">
        <span>Ground</span>
        <select value={ground} onChange={(e) => setGround(e.target.value as Ground)}>
          {GROUNDS.map((g) => (
            <option key={g.id} value={g.id}>
              {g.label}
            </option>
          ))}
        </select>
      </label>
      <p className="map-hint">
        Saves as assets/maps/{fileId}.json. Starts as an open floor with one room per team.
        {dirty && " Unsaved changes to the current map will be lost."}
      </p>
      {error && (
        <p className="map-notice error" role="alert">
          {error}
        </p>
      )}
      <div className="map-actions">
        <Button size="1" className="action-button" onClick={create}>
          Create map
        </Button>
        <Button size="1" className="action-button" variant="surface" color="gray" onClick={close}>
          Cancel
        </Button>
      </div>
    </section>
  );
}

function OpenMap({ dirty }: { dirty: boolean }) {
  const maps = useStore(mapList, (m) => m);
  const id = useStore(mapId, (i) => i);
  const source = useStore(mapSource, (s) => s);
  const name = useStore(mapHistory, (h) => h?.present.name ?? id);
  const [pending, setPending] = useState<string | null>(null);
  const open = (next: string) => {
    setPending(null);
    void loadMap(next);
  };
  // A map opened from the server is shown as such; picking a file loads it.
  const fromServer = `server:${id}`;
  return (
    <>
      <select
        aria-label="Open map"
        value={source === "server" ? fromServer : id}
        onChange={(e) => {
          const next = e.target.value;
          if (next === fromServer) return;
          dirty ? setPending(next) : open(next);
        }}
      >
        {source === "server" && (
          <option value={fromServer}>
            {name} ({id}, server)
          </option>
        )}
        {maps.map((m) => (
          <option key={m.id} value={m.id}>
            {m.name} ({m.id})
          </option>
        ))}
      </select>
      {pending && (
        <DiscardPrompt
          label={`Open ${pending}`}
          onOpen={() => open(pending)}
          onKeep={() => setPending(null)}
        />
      )}
    </>
  );
}

function AddProp({ map }: { map: ArenaMap }) {
  const catalog = useStore(propCatalog, (c) => c);
  const [model, setModel] = useState("Barrel");
  const chosen = catalog.some((c) => c.id === model) ? model : (catalog[0]?.id ?? "");
  return (
    <section className="map-inspector stack" aria-label="Add prop">
      <h3>Props</h3>
      <div className="map-props-add">
        <select
          aria-label="Prop model"
          value={chosen}
          disabled={!catalog.length}
          onChange={(e) => setModel(e.target.value)}
        >
          <PropOptions catalog={catalog} />
        </select>
        <Button
          size="1"
          variant="surface"
          disabled={!chosen}
          onClick={() => {
            const next = addProp(map, chosen);
            commitMap(next);
            mapSelection.set({ kind: "prop", index: next.props.length - 1 });
          }}
        >
          Add prop
        </Button>
      </div>
      {!catalog.length && (
        <p className="map-hint">The prop list did not load; run scripts/prepare_props.py.</p>
      )}
    </section>
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
  const serverSaved = useStore(serverSavedText, (s) => s);
  const source = useStore(mapSource, (s) => s);
  const file = useRef<HTMLInputElement>(null);
  const map = history?.present;
  const text = map ? serializeMap(map) : null;
  /** Differs from the map file (the local save button). */
  const dirty = !!map && saved !== text;
  /** Differs from both the file and the server copy: worth a warning. */
  const unsaved = dirty && serverSaved !== text;
  const status = useStore(mapStatus, (s) => s);
  const valid = status?.revision === currentRevision() && status.errors.length === 0;
  const id = useStore(mapId, (i) => i);
  const [creating, setCreating] = useState(false);
  const admin = useAdmin();

  useEffect(() => {
    if (!visible) return;
    void loadPropCatalog();
    void listMaps();
  }, [visible]);
  useEffect(() => {
    if (visible && admin) void listServerMaps();
  }, [visible, admin]);

  useEffect(() => {
    if (!unsaved) return;
    const warn = (e: BeforeUnloadEvent) => e.preventDefault();
    addEventListener("beforeunload", warn);
    return () => removeEventListener("beforeunload", warn);
  }, [unsaved]);

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
    } else if (e.key.toLowerCase() === "r" && !mod && selection?.kind === "prop") {
      handled();
      commitMap(rotateProp(map, selection.index, e.shiftKey ? -15 : 15));
    } else if (
      mod &&
      e.key.toLowerCase() === "d" &&
      (selection?.kind === "pillar" || selection?.kind === "prop")
    ) {
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
            <div className="map-toolbar" role="toolbar" aria-label="Map files">
              <OpenMap dirty={unsaved} />
              <Button size="1" variant="surface" onClick={() => setCreating(!creating)}>
                New map…
              </Button>
            </div>
            {creating && <NewMapForm dirty={dirty} close={() => setCreating(false)} />}
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
                  Save as {id}.json
                </Button>
              )}
              {admin && <ServerMaps dirty={unsaved} valid={valid} id={id} />}
              <Button size="1" variant="surface" color="gray" onClick={() => download(map, id)}>
                Download
              </Button>
              <Button size="1" variant="surface" color="gray" onClick={() => file.current?.click()}>
                Import…
              </Button>
              {source === "server" ? (
                <Button size="1" variant="surface" color="gray" onClick={() => openServerMap(id)}>
                  Reload from server
                </Button>
              ) : (
                <Button size="1" variant="surface" color="gray" onClick={() => loadMap()}>
                  Reload file
                </Button>
              )}
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
                  Drag pillars, rooms, spawns and props; corner and back handles resize. Alt: 0.1 m
                  snap. R turns the selected prop. Arrows nudge (Shift ×4), Delete removes, Ctrl+Z /
                  Ctrl+Shift+Z undo and redo. Edits go live in the arena; the round keeps going
                  unless a unit ends up inside a wall.
                  {writable === false &&
                    " Saving needs the local dev server; use Download instead."}
                </p>
              </div>
              <div className="map-side stack">
                <ItemList map={map} selection={selection} />
                <MapInspector map={map} selection={selection} />
                <AddProp map={map} />
              </div>
            </div>
          </>
        )}
      </div>
    </MovableWindow>
  );
}
