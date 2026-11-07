import { describe, expect, it } from "vitest";
import { newBar, place, removeAssignment } from "../../web/src/domain/action-bars/model";
import {
  bind,
  BindingResolver,
  chord,
  conflicts,
  defaults,
  unsupported,
} from "../../web/src/domain/bindings/model";
import { layout, move, position } from "../../web/src/domain/layout/model";
import { initial, parsePreferences, validateConfig } from "../../web/src/domain/preferences/schema";
import { importProfiles, profileTransition } from "../../web/src/domain/preferences/profiles";
import { Persistence } from "../../web/src/domain/preferences/persistence";
import { nextTarget, reconcileFocus } from "../../web/src/domain/units/selection";

describe("bindings", () => {
  it("requires explicit conflict replacement and preserves the original", () => {
    const bindings = defaults([newBar("one")]);
    const value = chord({ code: "KeyW" });
    expect(conflicts(bindings, value, "back", 1)).toEqual([{ action: "forward", entry: 0 }]);
    expect(bind(bindings, "back", 1, value)).toBe(bindings);
    const changed = bind(bindings, "back", 1, value, true);
    expect(changed.forward[0]).toBeNull();
    expect(changed.back[1]).toEqual(value);
    expect(bindings.forward[0]).not.toBeNull();
  });

  it("keeps a movement action held through alternate and unrelated release", () => {
    let bindings = defaults([newBar("one")]);
    bindings = bind(bindings, "forward", 1, chord({ code: "ArrowUp" }));
    const resolver = new BindingResolver();
    resolver.down(chord({ code: "KeyW" }), bindings);
    resolver.down(chord({ code: "ArrowUp" }), bindings);
    resolver.up("KeyW");
    resolver.up("KeyD");
    expect(resolver.active("forward")).toBe(true);
    resolver.up("ArrowUp");
    expect(resolver.active("forward")).toBe(false);
  });

  it("normalizes modifiers, ignores repeats and releases shifted holds", () => {
    const bindings = bind(
      defaults([newBar("one")]),
      "forward",
      1,
      chord({ code: "KeyE", shiftKey: true }),
    );
    const resolver = new BindingResolver();
    const key = chord({ code: "KeyE", shiftKey: true });
    expect(resolver.down(key, bindings)).toBe("forward");
    expect(resolver.down({ ...key, repeat: true }, bindings)).toBeNull();
    resolver.up("ShiftLeft");
    expect(resolver.active("forward")).toBe(false);
    resolver.down(key, bindings);
    resolver.clear();
    expect(resolver.active("forward")).toBe(false);
  });

  it.each(["F1", "ShiftLeft", "Unidentified"])("rejects unsupported key %s", (code) => {
    expect(unsupported(chord({ code }))).toBeTruthy();
  });

  it("preserves browser shortcuts", () => {
    expect(unsupported(chord({ code: "KeyR", ctrlKey: true }))).toContain("Reserved");
    expect(unsupported(chord({ code: "Tab", shiftKey: true }))).toContain("Reserved");
  });
});

describe("slot transactions", () => {
  it("removes only a matching source assignment and preserves slot bindings/identity", () => {
    const bars = [newBar("one")];
    const removed = removeAssignment(bars, "one-s1", "pulse");
    expect(removed[0].slots[0]).toEqual({ id: "one-s1", ability: null });
    expect(bars[0].slots[0].ability).toBe("pulse");
    expect(removeAssignment(bars, "missing", "pulse")).toBe(bars);
  });
  it("copies from catalog, moves, swaps and removes without mutation", () => {
    const bars = [newBar("one")];
    const copied = place(bars, "one-s2", { ability: "pulse" });
    expect(copied[0].slots[1].ability).toBe("pulse");
    const moved = place(bars, "one-s2", { ability: "pulse", source: "one-s1" });
    expect(moved[0].slots[0].ability).toBeNull();
    expect(moved[0].slots[1].ability).toBe("pulse");
    const swapped = place(copied, "one-s2", { ability: "pulse", source: "one-s1" });
    expect(swapped[0].slots[0].ability).toBe("pulse");
    const explicitCopy = place(bars, "one-s3", { ability: "pulse", source: "one-s1", copy: true });
    expect(explicitCopy[0].slots[0].ability).toBe("pulse");
    expect(bars[0].slots[1].ability).toBeNull();
  });

  it("treats stale, invalid and self drops as no-ops", () => {
    const bars = [newBar("one")];
    expect(place(bars, "missing", { ability: "pulse" })).toBe(bars);
    expect(place(bars, "one-s2", { ability: "pulse", source: "one-s3" })).toBe(bars);
    expect(place(bars, "one-s1", { ability: "pulse", source: "one-s1" })).toBe(bars);
  });
});

describe("layout and identity", () => {
  it("clamps movement and oversized viewport placement", () => {
    expect(move(layout(), -10000, 10000, 800, 600)).toMatchObject({ x: 0, y: 1 });
    expect(
      position(layout(1, 1), { width: 200, height: 100 }, { width: 300, height: 200 }),
    ).toEqual({ left: 0, top: 0 });
    expect(
      position(layout(0.5, 0.5), { width: 800, height: 600 }, { width: 200, height: 100 }),
    ).toEqual({ left: 300, top: 250 });
  });

  it("cycles deterministic identities without changing the input list", () => {
    const ids = ["z", "a"];
    expect(nextTarget(ids, null)).toBe("a");
    expect(nextTarget(ids, "a")).toBe("z");
    expect(nextTarget(ids, "z")).toBe("a");
    expect(nextTarget([], null)).toBeNull();
    expect(ids).toEqual(["z", "a"]);
  });

  it("keeps focus by identity, clears on disappearance and reset", () => {
    const units = [{ id: "enemy", name: "Enemy", health: 50, maxHealth: 100 }];
    expect(reconcileFocus("enemy", units)).toBe("enemy");
    expect(reconcileFocus("enemy", [{ ...units[0], health: 25 }])).toBe("enemy");
    expect(reconcileFocus("enemy", [])).toBeNull();
    expect(reconcileFocus("enemy", units, true)).toBeNull();
  });
});

describe("preferences and profiles", () => {
  it("starts unlocked and migrates the old clear-target Escape without overriding custom locks", () => {
    expect(initial().profiles[0].config.assignmentsLocked).toBe(false);
    const old = initial().profiles[0].config;
    delete old.bindings.menu;
    old.bindings.clearTarget = [chord({ code: "Escape" }), null];
    old.assignmentsLocked = true;
    const migrated = validateConfig(old);
    expect(migrated.bindings.menu[0]?.code).toBe("Escape");
    expect(migrated.bindings.clearTarget[0]).toEqual(chord({ code: "Escape", shiftKey: true }));
    expect(migrated.assignmentsLocked).toBe(true);
  });

  it("does not steal a custom Escape binding while adding the menu action", () => {
    const old = initial().profiles[0].config;
    delete old.bindings.menu;
    old.bindings.clearFocus = [chord({ code: "Escape" }), null];
    const migrated = validateConfig(old);
    expect(migrated.bindings.menu).toEqual([null, null]);
    expect(migrated.bindings.clearFocus[0]?.code).toBe("Escape");
  });
  it("round trips defaults and migrates the known pre-profile schema", () => {
    expect(parsePreferences(JSON.stringify(initial()))).toEqual(initial());
    expect(
      parsePreferences(JSON.stringify({ version: 0, config: initial().profiles[0].config }))
        .profiles[0].name,
    ).toBe("Migrated");
  });

  it("bounds corrupt layouts, removes stale abilities/actions, deduplicates slots", () => {
    const c = initial().profiles[0].config;
    const config = validateConfig({
      ...c,
      bars: [
        {
          ...c.bars[0],
          rows: 100,
          slots: [
            { id: "a", ability: "imaginary" },
            { id: "a", ability: "pulse" },
          ],
        },
      ],
      frames: { player: { x: -5, y: 100, scale: Infinity } },
      bindings: { fake: [] },
    });
    expect(config.bars[0].slots).toEqual([{ id: "a", ability: null }]);
    expect(config.bars[0].rows).toBe(1);
    expect(config.frames.player).toMatchObject({ x: 0, y: 1, scale: 1 });
    expect(config.bindings.fake).toBeUndefined();
  });

  it("rejects malformed, future, oversized and duplicate profile payloads", () => {
    expect(() => parsePreferences("{")).toThrow();
    expect(() => parsePreferences('{"version":99}')).toThrow("Unsupported");
    expect(() => parsePreferences("x".repeat(200001))).toThrow("large");
    const p = initial();
    p.profiles.push(p.profiles[0]);
    expect(() => parsePreferences(JSON.stringify(p))).toThrow("duplicate");
  });

  it("creates, switches, renames, duplicates, restores and deletes profiles", () => {
    let p = profileTransition(initial(), { type: "create", id: "two", name: "Second" });
    expect(p.active).toBe("two");
    p = profileTransition(p, { type: "rename", name: "Practice" });
    p = profileTransition(p, { type: "duplicate", id: "three", name: "Copy" });
    expect(p.profiles[2].config).toEqual(p.profiles[1].config);
    expect(p.profiles[2].config).not.toBe(p.profiles[1].config);
    p = profileTransition(p, { type: "select", id: "two" });
    p = profileTransition(p, { type: "restore" });
    p = profileTransition(p, { type: "delete", id: "two" });
    expect(p.active).toBe("default");
    expect(() => profileTransition(initial(), { type: "delete", id: "default" })).toThrow();
    expect(() =>
      profileTransition(initial(), { type: "create", id: "default", name: "Other" }),
    ).toThrow();
  });

  it("imports without overwriting profiles or duplicate names", () => {
    const p = importProfiles(initial(), initial(), () => "fresh");
    expect(p.profiles.map((v) => v.name)).toEqual(["Default", "Default (2)"]);
    expect(p.profiles.map((v) => v.id)).toEqual(["default", "fresh"]);
  });

  it("preserves invalid and unknown storage until explicit recovery", () => {
    let raw = '{"version":200}';
    const messages: string[] = [];
    const storage = {
      getItem: () => raw,
      setItem: (_: string, value: string) => {
        raw = value;
      },
    };
    const persistence = new Persistence(storage, (m) => messages.push(m));
    expect(persistence.load()).toEqual(initial());
    persistence.schedule(initial());
    persistence.flush();
    expect(raw).toBe('{"version":200}');
    persistence.enableWrites();
    expect(JSON.parse(raw).version).toBe(1);
    expect(messages[0]).toContain("preserved");
  });

  it("coalesces pending changes and reports quota/storage failures", () => {
    const saved: string[] = [],
      messages: string[] = [];
    const persistence = new Persistence(
      {
        getItem: () => null,
        setItem: (_, v) => {
          saved.push(v);
        },
      },
      (m) => messages.push(m),
    );
    persistence.schedule(initial());
    const latest = profileTransition(initial(), { type: "rename", name: "Latest" });
    persistence.schedule(latest);
    persistence.flush();
    expect(saved).toHaveLength(1);
    expect(JSON.parse(saved[0]).profiles[0].name).toBe("Latest");
    const unavailable = new Persistence(
      {
        getItem: () => {
          throw Error("denied");
        },
        setItem: () => {},
      },
      (m) => messages.push(m),
    );
    unavailable.load();
    expect(unavailable.blocked).toBe(true);
    const quota = new Persistence(
      {
        getItem: () => null,
        setItem: () => {
          throw Error("quota");
        },
      },
      (m) => messages.push(m),
    );
    quota.schedule(initial());
    quota.flush();
    expect(messages.at(-1)).toContain("Saving failed");
  });
});
