import {
  ABILITY_IDS,
  isAbilityId,
  newBar,
  MAX_BARS,
  MAX_SLOTS,
  type Bar,
} from "../action-bars/model";
import {
  actionList,
  defaults,
  unsupported,
  same,
  type Bindings,
  type Chord,
} from "../bindings/model";
import { barLayout, clamp, layout, unitFrameLayout, type Layout } from "../layout/model";
export type FrameRole =
  | "player"
  | "target"
  | "focus"
  | "opponents"
  | "team"
  | "unitTooltip"
  | "castBar"
  | "chat"
  | "launcher";
export type Config = {
  bars: Bar[];
  bindings: Bindings;
  frames: Record<FrameRole, Layout>;
  /** Show the player's cast bar inside the player frame instead of its own frame. */
  castBarAttached: boolean;
  /** The chat box's own width and height (px, before its layout scale). */
  chatSize: ChatSize;
  /** The chat box sits beside the launcher (its own `frames.chat` unused
   * until it is moved), filling the width up to the action bar. */
  chatAttached: boolean;
  /** The player set the chat width themselves (grip or slider): an
   * attached chat keeps it instead of filling the space. */
  chatSizeCustom: boolean;
  /** The player set the chat height themselves: an attached chat keeps it
   * instead of matching the launcher's height. */
  chatHeightCustom: boolean;
  assignmentsLocked: boolean;
  /** Show empty action bar slots; when false they appear only while a spell
   * is dragged (or the layout or key binding editors are open). */
  showEmptySlots: boolean;
  /** How early a spell may be pressed before the GCD or the current cast
   * ends and still be cast the moment it ends (ms; 0 = off; ADR 0058). */
  castBufferMs: number;
  nameplates: { friendly: boolean; enemy: boolean };
  /** The whole interface's size: every HUD frame's own scale is
   * multiplied by it, and nameplates and windows follow it. */
  uiScale: number;
};
/** The interface scale setting's range. */
export const UI_SCALE = { min: 0.6, max: 1.6, step: 0.05, initial: 1 } as const;
/** The cast buffer setting's range. The rules cap it at 12 ticks (400 ms). */
export const CAST_BUFFER_MS = { min: 0, max: 400, step: 50, initial: 400 } as const;
export type ChatSize = { width: number; height: number };
/** The chat box's size limits: big enough for tabs, a few lines and the
 * field; at most most of a large screen. */
export const CHAT_SIZE = {
  min: { width: 260, height: 150 },
  max: { width: 960, height: 720 },
  // 20% wider than the first default (360); the height follows the
  // launcher's beside it until the player sets one.
  initial: { width: 432, height: 210 },
} as const;
export function clampChatSize(size: ChatSize): ChatSize {
  return {
    width: Math.round(clamp(size.width, CHAT_SIZE.min.width, CHAT_SIZE.max.width)),
    height: Math.round(clamp(size.height, CHAT_SIZE.min.height, CHAT_SIZE.max.height)),
  };
}
export type Profile = { id: string; name: string; config: Config };
export type Preferences = { version: 1; active: string; profiles: Profile[] };
export const MAX_PROFILES = 8;
export function defaultConfig(): Config {
  const bars = [newBar("bar-1")];
  return {
    bars,
    bindings: defaults(bars),
    frames: {
      player: unitFrameLayout(0.03, 0.04),
      // Beside the 272 px player frame, about 30 px from it on a 1920 px
      // wide screen at 100% interface scale (positions are fractions of the
      // free width).
      target: unitFrameLayout(0.215, 0.04),
      focus: unitFrameLayout(0.7, 0.23),
      opponents: unitFrameLayout(0.97, 0.24),
      team: unitFrameLayout(0.03, 0.24),
      unitTooltip: layout(1, 1),
      // Centred, just above the default action bar (y 0.92), down to 800×600.
      castBar: layout(0.5, 0.78),
      // Bottom left, beside the launcher's default place: used once the chat
      // is detached from the launcher.
      chat: layout(0.04, 1),
      // Bottom-left corner; the chat sits beside it by default.
      launcher: layout(0, 1),
    },
    castBarAttached: false,
    chatSize: { ...CHAT_SIZE.initial },
    chatAttached: true,
    chatSizeCustom: false,
    chatHeightCustom: false,
    assignmentsLocked: false,
    showEmptySlots: true,
    castBufferMs: CAST_BUFFER_MS.initial,
    nameplates: { friendly: true, enemy: true },
    uiScale: UI_SCALE.initial,
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
  const l: Layout = {
    x: num(r.x, d.x, 0, 1),
    y: num(r.y, d.y, 0, 1),
    scale: num(r.scale, 1, 0.6, 1.6),
    visible: bool(r.visible, true),
    locked: bool(r.locked, false),
  };
  // Only frames that draw a background keep its opacity; profiles from
  // before the setting take the (more transparent) default.
  if (d.background !== undefined) l.background = num(r.background, d.background, 0, 1);
  if (d.portrait !== undefined) l.portrait = bool(r.portrait, d.portrait);
  return l;
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
          return [{ id: sid, ability: isAbilityId(s.ability) ? s.ability : null }];
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
        layout: cleanLayout(b.layout, barLayout()),
      });
    }
  const safeBars = bars.length ? bars : d.bars;
  const bindings = defaults(safeBars);
  const rawBindings = { ...record(r.bindings) };
  // Additive migration for saved single-spell profiles: populate slots 2-4 with
  // Rift Bolt, Quell, and Mend (and default Digit2-Digit4 if unbound/unused)
  // when none of the new spells have been placed yet and slots 2-4 are empty.
  const firstBar = safeBars[0];
  const hasNewSpells = safeBars.some((bar) =>
    bar.slots.some((slot) => slot.ability !== null && slot.ability !== "pulse"),
  );
  if (
    !hasNewSpells &&
    firstBar &&
    firstBar.slots.length >= 4 &&
    firstBar.slots[0].ability === "pulse" &&
    firstBar.slots.slice(1, 4).every((slot) => slot.ability === null)
  ) {
    const taken = (candidate: Chord | null) =>
      !!candidate &&
      Object.values(rawBindings).some(
        (pair) => Array.isArray(pair) && pair.some((c) => same(record(c) as Chord, candidate)),
      );
    for (let i = 1; i < ABILITY_IDS.length; i += 1) {
      firstBar.slots[i].ability = ABILITY_IDS[i];
      const action = `slot:${firstBar.slots[i].id}`;
      const existing = rawBindings[action];
      const emptyBinding =
        !Array.isArray(existing) ||
        existing.every((entry) => !entry || typeof record(entry).code !== "string");
      const fallback = bindings[action]?.[0] ?? null;
      if (emptyBinding && fallback && !taken(fallback)) {
        rawBindings[action] = [fallback, null];
      }
    }
  }
  // Newly introduced defaults must not steal existing custom chords.
  for (const action of ["toggleEnemyNameplates", "toggleFriendlyNameplates", "toggleSheath"]) {
    if (
      !(action in rawBindings) &&
      Object.values(rawBindings).some(
        (pair) =>
          Array.isArray(pair) && pair.some((c) => same(record(c) as Chord, bindings[action][0])),
      )
    )
      rawBindings[action] = [null, null];
  }
  // Autorun has two defaults; drop only the ones an existing profile already uses.
  if (!("autorun" in rawBindings)) {
    const taken = (d: Chord | null) =>
      Object.values(rawBindings).some(
        (pair) => Array.isArray(pair) && pair.some((c) => same(record(c) as Chord, d)),
      );
    rawBindings.autorun = bindings.autorun.map((d) => (taken(d) ? null : d));
  }
  // Existing custom Space assignments take precedence over the new default.
  if (
    !("jump" in rawBindings) &&
    Object.values(rawBindings).some(
      (pair) => Array.isArray(pair) && pair.some((c) => same(record(c) as Chord, bindings.jump[0])),
    )
  )
    rawBindings.jump = [null, null];
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
      target: movedDefault(
        cleanLayout(frames.target, d.frames.target),
        OLD_TARGET,
        d.frames.target,
      ),
      focus: cleanLayout(frames.focus, d.frames.focus),
      opponents: cleanLayout(frames.opponents, d.frames.opponents),
      team: cleanLayout(frames.team, d.frames.team),
      unitTooltip: cleanLayout(frames.unitTooltip, d.frames.unitTooltip),
      castBar: cleanLayout(frames.castBar, d.frames.castBar),
      chat:
        typeof r.chatAttached !== "boolean" && untouched(r, frames, d)
          ? d.frames.chat
          : cleanLayout(frames.chat, d.frames.chat),
      launcher: movedDefault(
        cleanLayout(frames.launcher, d.frames.launcher),
        OLD_LAUNCHER,
        d.frames.launcher,
      ),
    },
    castBarAttached: bool(r.castBarAttached, false),
    chatSize: newDefaultChatSize(cleanChatSize(r.chatSize), bool(r.chatSizeCustom, false)),
    // Profiles from before attaching: attached when the chat still sat at
    // an old default (the player never moved it), detached otherwise.
    chatAttached: typeof r.chatAttached === "boolean" ? r.chatAttached : untouched(r, frames, d),
    chatSizeCustom: bool(r.chatSizeCustom, false),
    // Profiles from before the flag: a height other than the old default
    // was the player's own (height-only resizes set no flag then).
    chatHeightCustom: bool(
      r.chatHeightCustom,
      cleanChatSize(r.chatSize).height !== OLD_CHAT_HEIGHT,
    ),
    assignmentsLocked: bool(r.assignmentsLocked, false),
    showEmptySlots: bool(r.showEmptySlots, true),
    castBufferMs: Math.round(
      num(r.castBufferMs, CAST_BUFFER_MS.initial, CAST_BUFFER_MS.min, CAST_BUFFER_MS.max),
    ),
    nameplates: {
      friendly: bool(record(r.nameplates).friendly, true),
      enemy: bool(record(r.nameplates).enemy, true),
    },
    uiScale: num(r.uiScale, UI_SCALE.initial, UI_SCALE.min, UI_SCALE.max),
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

/** The first default width, which profiles saved before it grew. */
const OLD_CHAT_WIDTH = 360;
/** The default height, which profiles from before `chatHeightCustom` saved. */
const OLD_CHAT_HEIGHT = 210;
/** A chat still at the first default width (never sized by the player)
 * takes the new default. */
function newDefaultChatSize(size: ChatSize, custom: boolean): ChatSize {
  return !custom && size.width === OLD_CHAT_WIDTH
    ? { ...size, width: CHAT_SIZE.initial.width }
    : size;
}
function cleanChatSize(raw: unknown): ChatSize {
  const value = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  const number = (n: unknown, fallback: number) =>
    typeof n === "number" && Number.isFinite(n) ? n : fallback;
  return clampChatSize({
    width: number(value.width, CHAT_SIZE.initial.width),
    height: number(value.height, CHAT_SIZE.initial.height),
  });
}

/** Defaults earlier versions saved into every profile. */
const OLD_LAUNCHER = [layout(0.99, 0.74), layout(1, 0.66)];
/** Earlier target defaults: beside the 236 px player frame, (briefly) far
 * to its right, then beside the 336 px frame. */
const OLD_TARGET = [layout(0.19, 0.04), layout(0.55, 0.04), layout(0.26, 0.04)];
const OLD_CHAT = [layout(0.01, 0.7), layout(0.01, 0.97)];
function sameLayout(a: Layout, b: Layout) {
  return (
    a.x === b.x &&
    a.y === b.y &&
    a.scale === b.scale &&
    a.visible === b.visible &&
    a.locked === b.locked
  );
}
/** A frame still at an old default moves to the new one; a frame the
 * player placed stays. */
function movedDefault(value: Layout, olds: Layout[], next: Layout) {
  return olds.some((old) => sameLayout(value, old)) ? next : value;
}
/** For profiles from before the attached chat: both the launcher and the
 * chat still sit at earlier defaults (the player moved neither), so the
 * chat can attach to the launcher's new place. */
function untouched(_r: Record<string, unknown>, frames: Record<string, unknown>, d: Config) {
  const launcher = cleanLayout(frames.launcher, d.frames.launcher);
  const chat = cleanLayout(frames.chat, d.frames.chat);
  return (
    OLD_LAUNCHER.some((old) => sameLayout(launcher, old)) &&
    OLD_CHAT.some((old) => sameLayout(chat, old))
  );
}
