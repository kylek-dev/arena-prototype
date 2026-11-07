import type { Bar } from "../action-bars/model";
export type Chord = { code: string; ctrl: boolean; alt: boolean; shift: boolean; meta: boolean };
export type Bindings = Record<string, [Chord | null, Chord | null]>;
export const movement = ["forward", "back", "left", "right"] as const;
export const actions = [
  ...movement,
  "nextTarget",
  "menu",
  "clearTarget",
  "setFocus",
  "clearFocus",
  "targetFocus",
  "reset",
  "settings",
  "spellbook",
  "quickBind",
  "layout",
];
export function actionList(bars: Bar[]) {
  return [...actions, ...bars.flatMap((b) => b.slots.map((s) => `slot:${s.id}`))];
}
const codes =
  /^(Key[A-Z]|Digit[0-9]|Numpad[0-9]|F([1-9]|1[0-2])|Arrow(Up|Down|Left|Right)|Space|Tab|Escape|Backquote|Minus|Equal|BracketLeft|BracketRight|Backslash|Semicolon|Quote|Comma|Period|Slash|Home|End|PageUp|PageDown|Insert|Delete)$/;
export function chord(e: {
  code: string;
  ctrlKey?: boolean;
  altKey?: boolean;
  shiftKey?: boolean;
  metaKey?: boolean;
}): Chord {
  return {
    code: e.code,
    ctrl: !!e.ctrlKey,
    alt: !!e.altKey,
    shift: !!e.shiftKey,
    meta: !!e.metaKey,
  };
}
export function unsupported(c: Chord): string | null {
  if (!codes.test(c.code)) return "Press a supported physical key, not a modifier alone.";
  if (c.meta || c.ctrl || c.alt || /^F/.test(c.code) || (c.code === "Tab" && c.shift))
    return "Reserved browser/OS shortcut. Use an unmodified key or Shift chord (not Shift+Tab).";
  return null;
}
export const same = (a: Chord | null, b: Chord | null) =>
  !!a &&
  !!b &&
  a.code === b.code &&
  a.ctrl === b.ctrl &&
  a.alt === b.alt &&
  a.shift === b.shift &&
  a.meta === b.meta;
export function label(c: Chord | null) {
  return c
    ? `${c.shift ? "Shift+" : ""}${c.code.replace("Key", "").replace("Digit", "")}`
    : "Unbound";
}
export function defaults(bars: Bar[]): Bindings {
  const result: Bindings = Object.fromEntries(actionList(bars).map((a) => [a, [null, null]]));
  for (const [action, code] of Object.entries({
    forward: "KeyW",
    back: "KeyS",
    left: "KeyA",
    right: "KeyD",
    nextTarget: "Tab",
    menu: "Escape",
    reset: "KeyR",
    settings: "KeyO",
    spellbook: "KeyP",
    setFocus: "KeyF",
  }))
    result[action] = [chord({ code }), null];
  result.clearTarget = [chord({ code: "Escape", shiftKey: true }), null];
  if (bars[0]?.slots[0]) result[`slot:${bars[0].slots[0].id}`] = [chord({ code: "Digit1" }), null];
  return result;
}
export function conflicts(bindings: Bindings, value: Chord, action: string, entry: number) {
  return Object.entries(bindings).flatMap(([a, pair]) =>
    pair.flatMap((c, i) =>
      same(c, value) && !(a === action && i === entry) ? [{ action: a, entry: i }] : [],
    ),
  );
}
export function bind(
  bindings: Bindings,
  action: string,
  entry: 0 | 1,
  value: Chord | null,
  replace = false,
): Bindings {
  if (!bindings[action] || (value && unsupported(value))) return bindings;
  const collisions = value ? conflicts(bindings, value, action, entry) : [];
  if (collisions.length && !replace) return bindings;
  const next = structuredClone(bindings);
  for (const c of collisions) next[c.action][c.entry] = null;
  next[action][entry] = value;
  return next;
}
// Physical keys own their actions until release. Releasing modifiers cancels
// modified holds; an unrelated release cannot stop a second binding's hold.
export class BindingResolver {
  held = new Map<string, { action: string; chord: Chord }>();
  down(
    e: KeyboardEvent | (ReturnType<typeof chord> & { repeat?: boolean }),
    bindings: Bindings,
  ): string | null {
    const c = "ctrlKey" in e ? chord(e as KeyboardEvent) : (e as Chord);
    if (unsupported(c) || e.repeat || this.held.has(c.code)) return null;
    const action = Object.keys(bindings).find((a) => bindings[a].some((v) => same(v, c)));
    if (!action) return null;
    this.held.set(c.code, { action, chord: c });
    return action;
  }
  up(code: string, shiftStillHeld = false) {
    this.held.delete(code);
    if (code.startsWith("Shift") && !shiftStillHeld)
      for (const [key, v] of this.held) if (v.chord.shift) this.held.delete(key);
  }
  active(action: string) {
    return [...this.held.values()].some((v) => v.action === action);
  }
  clear() {
    this.held.clear();
  }
}
