import { describe, expect, it } from "vitest";
import {
  castProgress,
  isValidActiveCast,
  isValidSpellKey,
  resourcePercent,
  spellLabel,
} from "../../web/src/domain/casting";

describe("casting and resource presentation domain helpers", () => {
  it("validates spell keys and active casts strictly", () => {
    for (const key of ["pulse", "rift_bolt", "quell", "mend"] as const) {
      expect(isValidSpellKey(key)).toBe(true);
      expect(spellLabel(key).length).toBeGreaterThan(0);
    }
    expect(isValidSpellKey("fireball")).toBe(false);
    expect(isValidActiveCast({ spell: "rift_bolt", remainingTicks: 20, totalTicks: 45 })).toBe(
      true,
    );
    for (const bad of [
      null,
      {},
      { spell: "unknown", remainingTicks: 10, totalTicks: 45 },
      { spell: "pulse", remainingTicks: 10, totalTicks: 45 },
      { spell: "quell", remainingTicks: 10, totalTicks: 45 },
      { spell: "rift_bolt", remainingTicks: 0, totalTicks: 45 },
      { spell: "rift_bolt", remainingTicks: 46, totalTicks: 45 },
      { spell: "rift_bolt", remainingTicks: 10, totalTicks: 0 },
      { spell: "rift_bolt", remainingTicks: 1.5, totalTicks: 45 },
      { spell: "rift_bolt", remainingTicks: 10, totalTicks: 1e30 },
    ]) {
      expect(isValidActiveCast(bad)).toBe(false);
    }
  });

  it("computes bounded cast progress, remaining seconds, and resource percentages", () => {
    expect(castProgress({ spell: "rift_bolt", remainingTicks: 45, totalTicks: 45 })).toBe(0);
    expect(castProgress({ spell: "rift_bolt", remainingTicks: 15, totalTicks: 45 })).toBeCloseTo(
      2 / 3,
    );
    expect(castProgress({ spell: "mend", remainingTicks: 1, totalTicks: 45 })).toBeCloseTo(44 / 45);
    expect(resourcePercent(65, 100)).toBe(65);
    expect(resourcePercent(120, 100)).toBe(100);
    expect(resourcePercent(-5, 100)).toBe(0);
    expect(resourcePercent(50, 0)).toBe(0);
  });
});
