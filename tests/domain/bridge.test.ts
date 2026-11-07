import { describe, expect, it } from "vitest";
import { adaptSnapshot } from "../../web/src/bridge/snapshot";
import { availability, canActivate } from "../../web/src/domain/abilities/catalog";
import { snapshot } from "../ui/fixtures";

describe("engine snapshot boundary", () => {
  it("adapts the two-entity practice ABI without inventing combat data", () => {
    const raw = { ...snapshot(), maxHealth: 150 };
    const adapted = adaptSnapshot(raw);
    expect(adapted.target.id).toBe("practice-opponent");
    expect(adapted.player.maxHealth).toBe(150);
    expect(adapted.pulse).toEqual(raw.pulse);
    expect(adapted.cooldown).toBe(raw.cooldown);
  });

  it.each([
    null,
    { ...snapshot(), maxHealth: 0 },
    { ...snapshot(), maxHealth: 100, pulse: undefined },
    { ...snapshot(), maxHealth: 100, cooldown: NaN },
    { ...snapshot(), maxHealth: 100, character: undefined },
    { ...snapshot(), maxHealth: 100, camera: undefined },
    { ...snapshot(), maxHealth: 100, hoveredUnit: "Team1" },
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
