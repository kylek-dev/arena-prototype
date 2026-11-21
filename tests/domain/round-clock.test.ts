import { describe, expect, it } from "vitest";
import {
  formatRoundClock,
  isRoundClockWarning,
  remainingRoundSeconds,
  roundResultLabel,
} from "../../web/src/domain/round-clock";

describe("round clock domain", () => {
  const MAX_TICKS = 20 * 60 * 30; // 36_000 ticks = 20:00 at 30 Hz

  it("computes remaining seconds from authoritative ticks without drift", () => {
    expect(remainingRoundSeconds(0, MAX_TICKS, 30)).toBe(1200);
    expect(remainingRoundSeconds(1, MAX_TICKS, 30)).toBe(1200);
    expect(remainingRoundSeconds(30, MAX_TICKS, 30)).toBe(1199);
    expect(remainingRoundSeconds(MAX_TICKS - 30, MAX_TICKS, 30)).toBe(1);
    expect(remainingRoundSeconds(MAX_TICKS - 1, MAX_TICKS, 30)).toBe(1);
    expect(remainingRoundSeconds(MAX_TICKS, MAX_TICKS, 30)).toBe(0);
    expect(remainingRoundSeconds(MAX_TICKS + 50, MAX_TICKS, 30)).toBe(0);
  });

  it("handles invalid or out-of-range inputs safely", () => {
    expect(remainingRoundSeconds(-5, 900, 30)).toBe(30);
    expect(remainingRoundSeconds(0, 0, 30)).toBe(0);
    expect(remainingRoundSeconds(0, 900, 0)).toBe(0);
    expect(remainingRoundSeconds(Number.NaN, 900, 30)).toBe(0);
  });

  it("formats MM:SS and flags the final warning minute", () => {
    expect(formatRoundClock(1200)).toBe("20:00");
    expect(formatRoundClock(605)).toBe("10:05");
    expect(formatRoundClock(60)).toBe("01:00");
    expect(formatRoundClock(9)).toBe("00:09");
    expect(formatRoundClock(0)).toBe("00:00");
    expect(formatRoundClock(-4)).toBe("00:00");

    expect(isRoundClockWarning(61)).toBe(false);
    expect(isRoundClockWarning(60)).toBe(true);
    expect(isRoundClockWarning(1)).toBe(true);
    expect(isRoundClockWarning(0)).toBe(false);
  });

  it("maps every authoritative round outcome to a player-facing label", () => {
    expect(roundResultLabel("won")).toBe("Victory");
    expect(roundResultLabel("lost")).toBe("Defeat");
    expect(roundResultLabel("draw")).toBe("Draw");
    expect(roundResultLabel("time_limit_draw")).toBe("Time's up · draw");
    expect(roundResultLabel(null)).toBeNull();
    expect(roundResultLabel(undefined)).toBeNull();
  });
});
