import { describe, expect, it } from "vitest";
import { adaptSnapshot, needsImmediateDelivery } from "../../web/src/bridge/snapshot";
import { availability, canActivate } from "../../web/src/domain/abilities/catalog";
import { snapshot, teamSnapshot } from "../ui/fixtures";
import { defaultConfig, validateConfig } from "../../web/src/domain/preferences/schema";

describe("engine snapshot boundary", () => {
  it("validates roster identity, counts, selection and health", () => {
    const valid = { ...teamSnapshot(3), maxHealth: 100 };
    expect(adaptSnapshot(valid).roster).toHaveLength(6);
    for (const invalid of [
      { ...valid, teamSize: 4 },
      { ...valid, selectedActor: 6 },
      { ...valid, selected: false },
      { ...valid, roster: valid.roster?.slice(1) },
      { ...valid, roster: valid.roster?.map((unit) => ({ ...unit, tag: "Player" })) },
      { ...valid, roster: valid.roster?.map((unit) => ({ ...unit, health: 101 })) },
    ])
      expect(() => adaptSnapshot(invalid)).toThrow();
  });

  it("adds left team placement to old profiles without moving existing frames", () => {
    const old = defaultConfig();
    const { team: _team, ...frames } = old.frames;
    const migrated = validateConfig({ ...old, frames });
    expect(migrated.frames.team.x).toBe(0.03);
    expect(migrated.frames.opponents).toEqual(old.frames.opponents);
    expect(validateConfig(migrated)).toEqual(migrated);
  });
  it("adapts the two-entity practice ABI without inventing combat data", () => {
    const raw = { ...snapshot(), maxHealth: 150 };
    const adapted = adaptSnapshot(raw);
    expect(adapted.target.id).toBe("practice-opponent");
    expect(adapted.player.maxHealth).toBe(150);
    expect(adapted.pulse).toEqual(raw.pulse);
    expect(adapted.cooldown).toBe(raw.cooldown);
  });

  it("validates round clock metadata and roundResult consistency", () => {
    const running = adaptSnapshot({
      ...snapshot(),
      maxHealth: 100,
      maxRoundTicks: 36_000,
      tickRate: 30,
      roundResult: null,
    });
    expect(running.maxRoundTicks).toBe(36_000);
    expect(running.tickRate).toBe(30);
    expect(running.roundResult).toBeNull();
    const timedOut = adaptSnapshot({
      ...snapshot({ won: true }),
      maxHealth: 100,
      maxRoundTicks: 36_000,
      tickRate: 30,
      roundResult: "time_limit_draw",
    });
    expect(timedOut.roundResult).toBe("time_limit_draw");
    expect(needsImmediateDelivery(running, timedOut)).toBe(true);
    expect(needsImmediateDelivery(running, { ...running, maxRoundTicks: 900 })).toBe(true);
    for (const invalid of [
      { ...snapshot(), maxHealth: 100, maxRoundTicks: 0 },
      { ...snapshot(), maxHealth: 100, maxRoundTicks: 1.5 },
      { ...snapshot(), maxHealth: 100, maxRoundTicks: 1e30 },
      { ...snapshot(), maxHealth: 100, tickRate: 0 },
      { ...snapshot(), maxHealth: 100, tickRate: 1e30 },
      { ...snapshot(), maxHealth: 100, roundResult: "forfeit" },
      { ...snapshot({ won: false }), maxHealth: 100, roundResult: "time_limit_draw" },
      { ...snapshot({ won: true }), maxHealth: 100, roundResult: null },
    ]) {
      expect(() => adaptSnapshot(invalid)).toThrow();
    }
  });

  it.each([
    null,
    { ...snapshot(), maxHealth: 0 },
    { ...snapshot(), maxHealth: 100, pulse: undefined },
    { ...snapshot(), maxHealth: 100, cooldown: NaN },
    { ...snapshot(), maxHealth: 100, character: undefined },
    { ...snapshot(), maxHealth: 100, camera: undefined },
    { ...snapshot(), maxHealth: 100, hoveredUnit: "Team4" },
    {
      ...snapshot(),
      maxHealth: 100,
      startup: { phase: "warming", ready: true, pending: 0, timedOut: false },
    },
    {
      ...snapshot(),
      maxHealth: 100,
      startup: { phase: "later", ready: false, pending: 0, timedOut: false },
    },
  ])("rejects incomplete engine data: %j", (raw) => {
    expect(() => adaptSnapshot(raw)).toThrow();
  });

  it("has explicit availability for every unsupported activation state", () => {
    expect(availability(null)).toBe("Loading arena");
    expect(availability(snapshot({ selected: false }))).toBe("Select a target");
    expect(availability(snapshot({ won: true }))).toBe("Round complete");
    expect(availability(snapshot({ cooldown: 0.6 }))).toContain("Recharging");
    expect(canActivate(snapshot({ attackStatus: "Out of range" }))).toBe(false);
    expect(canActivate(snapshot({ attackStatus: "Line of sight blocked" }))).toBe(false);
    expect(canActivate(snapshot())).toBe(true);
  });
});
