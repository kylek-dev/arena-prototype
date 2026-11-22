import { describe, expect, it } from "vitest";
import {
  formatRoundClock,
  isRoundClockWarning,
  remainingRoundSeconds,
  roundClockFromSeconds,
  roundClockView,
  roundResultBanner,
  roundResultLabel,
  snapshotRemainingSeconds,
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

  it("derives clock view from authoritative snapshot metadata and handles missing metadata", () => {
    expect(snapshotRemainingSeconds(null)).toBeNull();
    expect(snapshotRemainingSeconds({ tick: 0 })).toBeNull();
    expect(snapshotRemainingSeconds({ tick: 0, maxRoundTicks: MAX_TICKS })).toBeNull();
    expect(snapshotRemainingSeconds({ tick: 0, maxRoundTicks: 0, tickRate: 30 })).toBeNull();
    expect(snapshotRemainingSeconds({ tick: 0, maxRoundTicks: MAX_TICKS, tickRate: 0 })).toBeNull();
    expect(roundClockView(null)).toBeNull();
    expect(roundClockView({ tick: 0, won: false })).toBeNull();

    expect(
      roundClockView({
        tick: 0,
        maxRoundTicks: MAX_TICKS,
        tickRate: 30,
        won: false,
        roundResult: null,
      }),
    ).toEqual({ text: "20:00", seconds: 1200, warning: false, expired: false });

    // Inside final warning minute while still running.
    expect(
      roundClockView({
        tick: MAX_TICKS - 60 * 30,
        maxRoundTicks: MAX_TICKS,
        tickRate: 30,
        won: false,
        roundResult: null,
      }),
    ).toEqual({ text: "01:00", seconds: 60, warning: true, expired: false });

    // Finished during the final minute: timer stops warning.
    expect(
      roundClockView({
        tick: MAX_TICKS - 30 * 30,
        maxRoundTicks: MAX_TICKS,
        tickRate: 30,
        won: true,
        roundResult: "won",
      }),
    ).toEqual({ text: "00:30", seconds: 30, warning: false, expired: false });

    // Time limit reached.
    expect(
      roundClockView({
        tick: MAX_TICKS,
        maxRoundTicks: MAX_TICKS,
        tickRate: 30,
        won: true,
        roundResult: "time_limit_draw",
      }),
    ).toEqual({ text: "00:00", seconds: 0, warning: false, expired: true });

    // Explicit time_limit_draw marks expired even if remaining seconds > 0.
    expect(roundClockFromSeconds(5, true, "time_limit_draw")).toEqual({
      text: "00:05",
      seconds: 5,
      warning: false,
      expired: true,
    });

    // Elimination on the exact final tick (00:00) is not a time-limit expiry.
    expect(
      roundClockView({
        tick: MAX_TICKS,
        maxRoundTicks: MAX_TICKS,
        tickRate: 30,
        won: true,
        roundResult: "won",
      }),
    ).toEqual({ text: "00:00", seconds: 0, warning: false, expired: false });

    // Non-finite seconds clamp cleanly to 00:00 expired while running.
    expect(roundClockFromSeconds(Number.NaN, false, null)).toEqual({
      text: "00:00",
      seconds: 0,
      warning: false,
      expired: true,
    });
  });

  it("builds round result banners for practice and networked matches", () => {
    expect(roundResultBanner(null, false)).toBeNull();
    expect(roundResultBanner(undefined, true)).toBeNull();
    expect(roundResultBanner("won", false)).toEqual({
      label: "Victory",
      detail: "Round complete",
      tone: "victory",
    });
    expect(roundResultBanner("lost", true)).toEqual({
      label: "Defeat",
      detail: "Match closing shortly",
      tone: "defeat",
    });
    expect(roundResultBanner("draw", true)).toEqual({
      label: "Draw",
      detail: "Match closing shortly",
      tone: "draw",
    });
    expect(roundResultBanner("time_limit_draw", false)).toEqual({
      label: "Time's up · draw",
      detail: "Round time limit reached",
      tone: "time-limit",
    });
    expect(roundResultBanner("time_limit_draw", true)).toEqual({
      label: "Time's up · draw",
      detail: "Round time limit reached · match closing shortly",
      tone: "time-limit",
    });
  });
});
