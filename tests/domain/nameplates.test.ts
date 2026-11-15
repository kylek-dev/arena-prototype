import { expect, it } from "vitest";
import { adaptSnapshot } from "../../web/src/bridge/snapshot";
import { chord } from "../../web/src/domain/bindings/model";
import {
  defaultConfig,
  validateConfig,
  initial,
  parsePreferences,
} from "../../web/src/domain/preferences/schema";
import { visibleNameplates } from "../../web/src/domain/units/nameplates";
import { unitRoster } from "../../web/src/runtime/commands";
import { snapshot } from "../ui/fixtures";

it("filters allies/enemies independently, never shows self, and ignores absent units", () => {
  const state = snapshot();
  const roster = {
    ...unitRoster(state, null),
    team: [{ ...state.player, id: "ally", name: "Ally" }, state.player],
  };
  const plates = [
    { tag: "Opponent1" as const, x: 0.2, y: 0.3 },
    { tag: "Team1" as const, x: 0.4, y: 0.3 },
    { tag: "Team2" as const, x: 0.6, y: 0.3 },
    { tag: "Team3" as const, x: 0.8, y: 0.3 },
  ];
  expect(
    visibleNameplates(plates, roster, { friendly: true, enemy: true }).map((p) => p.unit.id),
  ).toEqual([state.target.id, "ally"]);
  expect(
    visibleNameplates(plates, roster, { friendly: true, enemy: false }).map((p) => p.unit.id),
  ).toEqual(["ally"]);
  expect(
    visibleNameplates(plates, roster, { friendly: false, enemy: true }).map((p) => p.unit.id),
  ).toEqual([state.target.id]);
  expect(visibleNameplates(plates, roster, { friendly: false, enemy: false })).toEqual([]);
});

it("validates projection bounds/identities and accepts older engines", () => {
  const raw = { ...snapshot(), maxHealth: 100 };
  expect(adaptSnapshot(raw).nameplates).toBeUndefined();
  const valid = { tag: "Opponent1", x: 0.5, y: 0.5 };
  expect(adaptSnapshot({ ...raw, nameplates: [valid] }).nameplates).toEqual([valid]);
  for (const plates of [
    null,
    [null],
    [{ ...valid, tag: "Player" }],
    [{ ...valid, x: NaN }],
    [{ ...valid, x: -0.1 }],
    [{ ...valid, y: 1.1 }],
    [valid, valid],
  ])
    expect(() => adaptSnapshot({ ...raw, nameplates: plates })).toThrow();
});

it("migrates defaults without stealing custom V/Shift+V and round-trips visibility", () => {
  const old = defaultConfig();
  delete old.bindings.toggleEnemyNameplates;
  delete old.bindings.toggleFriendlyNameplates;
  const migrated = validateConfig(old);
  expect(migrated.bindings.toggleEnemyNameplates[0]).toEqual(chord({ code: "KeyV" }));
  expect(migrated.bindings.toggleFriendlyNameplates[0]).toEqual(
    chord({ code: "KeyV", shiftKey: true }),
  );
  old.bindings["slot:bar-1-s1"] = [
    chord({ code: "KeyV" }),
    chord({ code: "KeyV", shiftKey: true }),
  ];
  const custom = validateConfig(old);
  expect(custom.bindings.toggleEnemyNameplates).toEqual([null, null]);
  expect(custom.bindings.toggleFriendlyNameplates).toEqual([null, null]);
  expect(custom.bindings["slot:bar-1-s1"]).toEqual(old.bindings["slot:bar-1-s1"]);
  expect(validateConfig({}).nameplates).toEqual({ friendly: true, enemy: true });
  const p = initial();
  p.profiles[0].config.nameplates = { friendly: false, enemy: false };
  expect(parsePreferences(JSON.stringify(p)).profiles[0].config.nameplates).toEqual({
    friendly: false,
    enemy: false,
  });
});
