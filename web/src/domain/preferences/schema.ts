import { newBar, MAX_BARS, MAX_SLOTS, type Bar } from "../action-bars/model";
import {
  actionList,
  defaults,
  unsupported,
  same,
  type Bindings,
  type Chord,
} from "../bindings/model";
import { clamp, layout, type Layout } from "../layout/model";
export type FrameRole = "player" | "target" | "focus";
export type Config = {
  bars: Bar[];
  bindings: Bindings;
  frames: Record<FrameRole, Layout>;
  assignmentsLocked: boolean;
};
export type Profile = { id: string; name: string; config: Config };
export type Preferences = { version: 1; active: string; profiles: Profile[] };
export const MAX_PROFILES = 8;
export function defaultConfig(): Config {
  const bars = [newBar("bar-1")];
  return {
    bars,
    bindings: defaults(bars),
    frames: { player: layout(0.03, 0.15), target: layout(0.97, 0.15), focus: layout(0.97, 0.53) },
    assignmentsLocked: false,
  };
}
export function initial(): Preferences {
  return {
    version: 1,
    active: "default",
    profiles: [{ id: "default", name: "Default", config: defaultConfig() }],
  };
}
const record = (v: unknown): Record<string, unknown> =>
  typeof v === "object" && v !== null && !Array.isArray(v) ? (v as Record<string, unknown>) : {};
const num = (v: unknown, fallback: number, min: number, max: number) =>
  typeof v === "number" && Number.isFinite(v) ? clamp(v, min, max) : fallback;
const bool = (v: unknown, fallback: boolean) => (typeof v === "boolean" ? v : fallback);
const id = (v: unknown) => (typeof v === "string" && /^[a-zA-Z0-9_-]{1,64}$/.test(v) ? v : null);
function cleanLayout(v: unknown, d: Layout): Layout {
  const r = record(v);
  return {
    x: num(r.x, d.x, 0, 1),
    y: num(r.y, d.y, 0, 1),
    scale: num(r.scale, 1, 0.6, 1.6),
    visible: bool(r.visible, true),
    locked: bool(r.locked, false),
  };
}
export function validateConfig(value: unknown): Config {
  const r = record(value),
    d = defaultConfig();
  const used = new Set<string>();
  const bars: Bar[] = [];
  if (Array.isArray(r.bars))
    for (const raw of r.bars.slice(0, MAX_BARS)) {
      const b = record(raw),
        bid = id(b.id);
      if (!bid || used.has(bid)) continue;
      used.add(bid);
      const slots = (Array.isArray(b.slots) ? b.slots : [])
        .slice(0, MAX_SLOTS)
        .flatMap((rawSlot) => {
          const s = record(rawSlot),
            sid = id(s.id);
          if (!sid || used.has(sid)) return [];
          used.add(sid);
          return [{ id: sid, ability: s.ability === "pulse" ? ("pulse" as const) : null }];
        });
      if (!slots.length) {
        let suffix = 1;
        let recovered = `${bid.slice(0, 40)}-recovered`;
        while (used.has(recovered)) recovered = `${bid.slice(0, 40)}-recovered-${suffix++}`;
        used.add(recovered);
        slots.push({ id: recovered, ability: null });
      }
      bars.push({
        id: bid,
        slots,
        rows: Math.round(num(b.rows, 1, 1, slots.length)),
        vertical: bool(b.vertical, false),
        layout: cleanLayout(b.layout, layout(0.5, 0.92)),
      });
    }
  const safeBars = bars.length ? bars : d.bars;
  const bindings = defaults(safeBars);
  const rawBindings = { ...record(r.bindings) };
  // Additive migration for profiles saved before the Escape menu existed.
  // Move only the old default clear-target Escape; preserve other custom uses.
  if (!("menu" in rawBindings)) {
    const isEscape = (value: unknown) => {
      const c = record(value);
      return c.code === "Escape" && !c.ctrl && !c.alt && !c.shift && !c.meta;
    };
    const clear = rawBindings.clearTarget;
    if (Array.isArray(clear) && clear.some(isEscape)) {
      rawBindings.clearTarget = clear.map((c) =>
        isEscape(c) ? defaults(safeBars).clearTarget[0] : c,
      );
    }
    const customEscape = Object.entries(rawBindings).some(
      ([action, pair]) => action !== "clearTarget" && Array.isArray(pair) && pair.some(isEscape),
    );
    if (customEscape) rawBindings.menu = [null, null];
  }
  const seen: Chord[] = [];
  for (const action of actionList(safeBars)) {
    const pair = rawBindings[action];
    if (!Array.isArray(pair)) continue;
    bindings[action] = [0, 1].map((i) => {
      const c = record(pair[i]);
      const v: Chord = {
        code: typeof c.code === "string" ? c.code : "",
        ctrl: c.ctrl === true,
        alt: c.alt === true,
        shift: c.shift === true,
        meta: c.meta === true,
      };
      if (unsupported(v) || seen.some((s) => same(s, v))) return null;
      seen.push(v);
      return v;
    }) as Bindings[string];
  }
  // Defaults for missing actions can also collide with imported keys.
  seen.length = 0;
  for (const action of actionList(safeBars))
    bindings[action] = bindings[action].map((c) => {
      if (!c || seen.some((s) => same(s, c))) return null;
      seen.push(c);
      return c;
    }) as Bindings[string];
  const frames = record(r.frames);
  return {
    bars: safeBars,
    bindings,
    frames: {
      player: cleanLayout(frames.player, d.frames.player),
      target: cleanLayout(frames.target, d.frames.target),
      focus: cleanLayout(frames.focus, d.frames.focus),
    },
    assignmentsLocked: bool(r.assignmentsLocked, false),
  };
}
export function parsePreferences(text: string): Preferences {
  if (text.length > 200_000) throw new Error("Profile file is too large (200 KB maximum).");
  const r = record(JSON.parse(text));
  // Version 0 was the pre-profile single-configuration export.
  if (r.version === 0)
    return {
      version: 1,
      active: "migrated",
      profiles: [{ id: "migrated", name: "Migrated", config: validateConfig(r.config) }],
    };
  if (r.version !== 1)
    throw new Error("Unsupported preference version; original storage has been preserved.");
  if (!Array.isArray(r.profiles) || !r.profiles.length || r.profiles.length > MAX_PROFILES)
    throw new Error("Expected 1–8 local profiles.");
  const used = new Set<string>();
  const profiles = r.profiles.map((v) => {
    const p = record(v),
      pid = id(p.id);
    if (!pid || used.has(pid)) throw new Error("Invalid or duplicate profile ID.");
    used.add(pid);
    return {
      id: pid,
      name: typeof p.name === "string" ? p.name.trim().slice(0, 40) || "Unnamed" : "Unnamed",
      config: validateConfig(p.config),
    };
  });
  return {
    version: 1,
    active: profiles.some((p) => p.id === r.active) ? (r.active as string) : profiles[0].id,
    profiles,
  };
}
