import { useEffect, useState } from "react";
import { Button } from "@radix-ui/themes";
import {
  addSpawn,
  duplicate,
  EDGES,
  GROUNDS,
  removeSelected,
  rotateProp,
  setFloor,
  setGround,
  setName,
  setVenue,
  TEAMS,
  updatePillar,
  updateProp,
  updateRoom,
  updateSpawn,
  venueFields,
  type ArenaMap,
  type Edge,
  type Ground,
  type MapProp,
  type PropInfo,
  type Selection,
  type Team,
} from "../../domain/map";
import { commitMap, mapSelection, propCatalog } from "../../state/map-editor";
import { useStore } from "../../state/store";

/** Catalog entries grouped by category, in catalog order. */
export function groupedProps(catalog: readonly PropInfo[]) {
  const groups = new Map<string, PropInfo[]>();
  for (const info of catalog) {
    const list = groups.get(info.category) ?? [];
    list.push(info);
    groups.set(info.category, list);
  }
  return [...groups.entries()];
}

/** `<option>`s for every prop, grouped by category. */
export function PropOptions({ catalog }: { catalog: readonly PropInfo[] }) {
  return (
    <>
      {groupedProps(catalog).map(([category, list]) => (
        <optgroup key={category} label={category}>
          {list.map((info) => (
            <option key={info.id} value={info.id}>
              {info.label}
            </option>
          ))}
        </optgroup>
      ))}
    </>
  );
}

function PropInspector({ map, index }: { map: ArenaMap; index: number }) {
  const catalog = useStore(propCatalog, (c) => c);
  const prop = map.props[index];
  const set = (patch: Partial<MapProp>) => commitMap(updateProp(map, index, patch));
  const known = catalog.some((c) => c.id === prop.model);
  return (
    <section className="map-inspector stack" aria-label="Selected prop">
      <h3>
        Prop {index + 1} · {catalog.find((c) => c.id === prop.model)?.label ?? prop.model}
      </h3>
      <label className="map-field map-field-wide">
        <span>Model</span>
        <select value={prop.model} onChange={(e) => set({ model: e.target.value })}>
          {!known && <option value={prop.model}>{prop.model}</option>}
          <PropOptions catalog={catalog} />
        </select>
      </label>
      <NumberField label="X" value={prop.x} step={0.5} onCommit={(x) => set({ x })} />
      <NumberField label="Z" value={prop.z} step={0.5} onCommit={(z) => set({ z })} />
      <NumberField label="Height" value={prop.y} step={0.1} onCommit={(y) => set({ y })} />
      <NumberField
        label="Turn"
        value={prop.rotation}
        step={15}
        unit="°"
        onCommit={(rotation) => set({ rotation })}
      />
      <NumberField
        label="Scale"
        value={prop.scale}
        step={0.1}
        unit="×"
        onCommit={(scale) => set({ scale })}
      />
      <p className="map-hint">
        Decoration only: fighters and spells pass through props. R turns 15° (Shift+R back).
      </p>
      <div className="map-actions">
        <Button
          variant="surface"
          color="gray"
          onClick={() => commitMap(rotateProp(map, index, 90))}
        >
          Turn 90°
        </Button>
        <Button
          variant="surface"
          color="gray"
          onClick={() => {
            const next = duplicate(map, { kind: "prop", index });
            commitMap(next.map);
            mapSelection.set(next.selection);
          }}
        >
          Duplicate
        </Button>
        <Button
          color="red"
          variant="soft"
          onClick={() => commitMap(removeSelected(map, { kind: "prop", index }))}
        >
          Delete prop
        </Button>
      </div>
    </section>
  );
}

/** Number input that commits on Enter or blur (one rebuild per edit), and reverts on Escape. */
export function NumberField({
  label,
  value,
  step,
  unit = "m",
  onCommit,
}: {
  label: string;
  value: number;
  step: number;
  unit?: string;
  onCommit: (value: number) => void;
}) {
  const [text, setText] = useState(String(value));
  useEffect(() => setText(String(value)), [value]);
  const apply = () => {
    const next = Number(text);
    if (text.trim() !== "" && Number.isFinite(next)) {
      if (next !== value) onCommit(next);
    } else setText(String(value));
  };
  return (
    <label className="map-field">
      <span>{label}</span>
      <input
        type="number"
        step={step}
        value={text}
        onChange={(e) => setText(e.target.value)}
        onBlur={apply}
        // Spinner clicks and arrow-key steps apply at once; typed text
        // applies on Enter or blur.
        onPointerUp={apply}
        onKeyUp={(e) => {
          if (e.key === "ArrowUp" || e.key === "ArrowDown") apply();
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") apply();
          if (e.key === "Escape" && text !== String(value)) {
            e.stopPropagation();
            setText(String(value));
          }
        }}
      />
      <span className="map-unit">{unit}</span>
    </label>
  );
}

function TextField({
  label,
  value,
  onCommit,
}: {
  label: string;
  value: string;
  onCommit: (v: string) => void;
}) {
  const [text, setText] = useState(value);
  useEffect(() => setText(value), [value]);
  return (
    <label className="map-field map-field-wide">
      <span>{label}</span>
      <input
        value={text}
        onChange={(e) => setText(e.target.value)}
        onBlur={() => text !== value && onCommit(text)}
        onKeyDown={(e) => e.key === "Enter" && text !== value && onCommit(text)}
      />
    </label>
  );
}

export function MapInspector({ map, selection }: { map: ArenaMap; selection: Selection }) {
  const remove = (label: string) => (
    <Button color="red" variant="soft" onClick={() => commitMap(removeSelected(map, selection))}>
      Delete {label}
    </Button>
  );
  if (selection?.kind === "pillar") {
    const i = selection.index;
    const p = map.pillars[i];
    const set = (patch: Partial<typeof p>) => commitMap(updatePillar(map, i, patch));
    return (
      <section className="map-inspector stack" aria-label="Selected pillar">
        <h3>Pillar {i + 1}</h3>
        <NumberField label="Centre X" value={p.x} step={0.5} onCommit={(x) => set({ x })} />
        <NumberField label="Centre Z" value={p.z} step={0.5} onCommit={(z) => set({ z })} />
        <NumberField
          label="Width (X)"
          value={p.width}
          step={0.5}
          onCommit={(width) => set({ width })}
        />
        <NumberField
          label="Depth (Z)"
          value={p.depth}
          step={0.5}
          onCommit={(depth) => set({ depth })}
        />
        <div className="map-actions">
          <Button
            variant="surface"
            color="gray"
            onClick={() => {
              const next = duplicate(map, selection);
              commitMap(next.map);
              mapSelection.set(next.selection);
            }}
          >
            Duplicate
          </Button>
          {remove("pillar")}
        </div>
      </section>
    );
  }
  if (selection?.kind === "prop") return <PropInspector map={map} index={selection.index} />;
  if (selection?.kind === "room") {
    const i = selection.index;
    const room = map.rooms[i];
    const set = (patch: Parameters<typeof updateRoom>[2]) => commitMap(updateRoom(map, i, patch));
    return (
      <section className="map-inspector stack" aria-label="Selected room">
        <h3>Room {i + 1}</h3>
        <label className="map-field map-field-wide">
          <span>Team</span>
          <select value={room.team} onChange={(e) => set({ team: e.target.value as Team })}>
            {TEAMS.map((team) => (
              <option key={team} value={team}>
                {team === "friendly" ? "Friendly (team 1)" : "Opposing (team 2)"}
              </option>
            ))}
          </select>
        </label>
        <label className="map-field map-field-wide">
          <span>Floor edge</span>
          <select value={room.edge} onChange={(e) => set({ edge: e.target.value as Edge })}>
            {EDGES.map((edge) => (
              <option key={edge} value={edge}>
                {edge[0].toUpperCase() + edge.slice(1)}
              </option>
            ))}
          </select>
        </label>
        <NumberField
          label="Along edge"
          value={room.offset}
          step={0.5}
          onCommit={(offset) => set({ offset })}
        />
        <NumberField
          label="Width (= door)"
          value={room.width}
          step={0.5}
          onCommit={(width) => set({ width })}
        />
        <NumberField
          label="Depth"
          value={room.depth}
          step={0.5}
          onCommit={(depth) => set({ depth })}
        />
        <NumberField
          label="Ceiling"
          value={room.ceiling}
          step={0.5}
          onCommit={(ceiling) => set({ ceiling })}
        />
        <p className="map-hint">
          Spawns: {room.spawns.length}. Drag them in the plan or select one to type positions.
        </p>
        <div className="map-actions">
          <Button variant="surface" color="gray" onClick={() => commitMap(addSpawn(map, i))}>
            Add spawn
          </Button>
          {remove("room")}
        </div>
      </section>
    );
  }
  if (selection?.kind === "spawn") {
    const room = map.rooms[selection.room];
    const spawn = room.spawns[selection.index];
    const set = (patch: Partial<typeof spawn>) =>
      commitMap(updateSpawn(map, selection.room, selection.index, patch));
    return (
      <section className="map-inspector stack" aria-label="Selected spawn">
        <h3>
          Room {selection.room + 1} spawn {selection.index + 1}
        </h3>
        <NumberField
          label="Across"
          value={spawn.across}
          step={0.5}
          onCommit={(across) => set({ across })}
        />
        <NumberField
          label="Back from door"
          value={spawn.back}
          step={0.5}
          onCommit={(back) => set({ back })}
        />
        <p className="map-hint">
          Players fill spawns in room order: team member 1 uses its first spawn.
        </p>
        <div className="map-actions">
          <Button
            variant="surface"
            color="gray"
            onClick={() => mapSelection.set({ kind: "room", index: selection.room })}
          >
            Select room
          </Button>
          {remove("spawn")}
        </div>
      </section>
    );
  }
  return (
    <section className="map-inspector stack" aria-label="Map settings">
      <h3>Map</h3>
      <TextField label="Name" value={map.name} onCommit={(name) => commitMap(setName(map, name))} />
      <label className="map-field map-field-wide">
        <span>Ground</span>
        <select
          value={map.ground}
          onChange={(e) => commitMap(setGround(map, e.target.value as Ground))}
        >
          {GROUNDS.map((g) => (
            <option key={g.id} value={g.id}>
              {g.label}
            </option>
          ))}
        </select>
      </label>
      <NumberField
        label="Floor width (X)"
        value={map.floor.width}
        step={1}
        onCommit={(v) => commitMap(setFloor(map, "width", v))}
      />
      <NumberField
        label="Floor depth (Z)"
        value={map.floor.depth}
        step={1}
        onCommit={(v) => commitMap(setFloor(map, "depth", v))}
      />
      <h3>Venue</h3>
      {venueFields.map((field) => (
        <NumberField
          key={field.key}
          label={field.label}
          value={map.venue[field.key]}
          step={field.step}
          unit={field.integer ? "" : "m"}
          onCommit={(v) => commitMap(setVenue(map, field.key, v))}
        />
      ))}
    </section>
  );
}
