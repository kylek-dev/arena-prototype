// The hub for the renderer (ADR 0056): what `readHub` hands the Bevy client
// each frame (where this player is and everyone in view with the render
// spec of their look), and where the client says each name goes on screen.

import { selectKit, renderSpec } from "../domain/appearance";
import { appearanceFromWire, type HubState, type WireAppearanceJson } from "../domain/hub";
import { characterKits } from "./appearance";
import { matchQueue } from "./network";
import { createStore } from "./store";

let revision = 0;
let cache: { hub: HubState | null; kits: unknown; json: string } | null = null;
/** Render specs by look, so a crowd costs one spec per look; reset when the
 * installed kits change. */
let specs = new WeakMap<WireAppearanceJson, unknown>();
let specKits: unknown = null;

function specFor(wire: WireAppearanceJson) {
  if (specKits !== characterKits.get()) {
    specKits = characterKits.get();
    specs = new WeakMap();
  }
  if (!specs.has(wire)) {
    const look = appearanceFromWire(wire);
    const kit = look ? selectKit(characterKits.get(), look) : undefined;
    specs.set(wire, kit && look ? renderSpec(kit, look) : null);
  }
  return specs.get(wire) ?? null;
}

/** `[revision, json]`: the hub, or `null` outside it; the revision changes
 * only when the hub or the installed kits do. */
export function readHub(): unknown[] {
  const hub = matchQueue.get()?.hub.get() ?? null;
  const kits = characterKits.get();
  if (!cache || cache.hub !== hub || cache.kits !== kits) {
    const json =
      hub?.status === "in"
        ? JSON.stringify({
            x: hub.x,
            z: hub.z,
            ...hub.body,
            tick: hub.tick,
            // The last move the door applied: the renderer replays the rest.
            ack: hub.ack,
            players: [...hub.players.values()].map((player) => ({
              id: player.id,
              x: player.x,
              z: player.z,
              facing: player.facing,
              height: player.height,
              admin: player.admin,
              spec: specFor(player.appearance),
            })),
          })
        : "null";
    if (json !== cache?.json) revision += 1;
    cache = { hub, kits, json };
  }
  return [revision, cache.json];
}

/** One numbered hub tick of the renderer's walk (and jump). */
export function onHubIntent(seq: number, x: number, z: number, facing: number, jump?: boolean) {
  matchQueue.get()?.moveHub(seq, x, z, facing, jump === true);
}

export type HubAnchor = { id: number; x: number; y: number };
/** Where each hub player's name goes, in viewport fractions. */
export const hubScreen = createStore<HubAnchor[]>([]);

export function onHubScreen(raw: string) {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return;
  }
  if (!Array.isArray(parsed)) return;
  const anchors: HubAnchor[] = [];
  for (const entry of parsed) {
    if (
      Array.isArray(entry) &&
      entry.length === 3 &&
      Number.isInteger(entry[0]) &&
      entry.slice(1).every((n) => typeof n === "number" && n >= 0 && n <= 1)
    )
      anchors.push({ id: entry[0], x: entry[1], y: entry[2] });
  }
  const previous = hubScreen.get();
  // Unchanged anchors (a still camera) keep the store quiet.
  if (
    previous.length === anchors.length &&
    previous.every((a, i) => a.id === anchors[i].id && a.x === anchors[i].x && a.y === anchors[i].y)
  )
    return;
  hubScreen.set(anchors);
}
