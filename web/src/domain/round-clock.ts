// Authoritative round-clock formatting and result labels (no wall clock or DOM).
import type { RoundResult } from "../bridge/snapshot";

/** Remaining whole seconds from the authoritative tick and round limit.
 * Rounds up so 20:00 displays at tick 0, 00:01 holds through the last second,
 * and 00:00 appears once `tick >= maxRoundTicks`. */
export function remainingRoundSeconds(
  tick: number,
  maxRoundTicks: number,
  tickRate: number,
): number {
  if (
    !Number.isFinite(tick) ||
    !Number.isFinite(maxRoundTicks) ||
    !Number.isFinite(tickRate) ||
    maxRoundTicks <= 0 ||
    tickRate <= 0
  ) {
    return 0;
  }
  const remainingTicks = Math.max(0, maxRoundTicks - Math.max(0, tick));
  return Math.ceil(remainingTicks / tickRate);
}

/** Formats non-negative whole seconds as `MM:SS`. */
export function formatRoundClock(seconds: number): string {
  const whole = Number.isFinite(seconds) ? Math.max(0, Math.floor(seconds)) : 0;
  const minutes = Math.floor(whole / 60);
  const secs = whole % 60;
  return `${String(minutes).padStart(2, "0")}:${String(secs).padStart(2, "0")}`;
}

/** Whether the remaining round clock is inside the final warning minute. */
export function isRoundClockWarning(seconds: number): boolean {
  return Number.isFinite(seconds) && seconds > 0 && seconds <= 60;
}

/** Player-facing summary for an authoritative round outcome. */
export function roundResultLabel(result: RoundResult | null | undefined): string | null {
  switch (result) {
    case "won":
      return "Victory";
    case "lost":
      return "Defeat";
    case "draw":
      return "Draw";
    case "time_limit_draw":
      return "Time's up · draw";
    default:
      return null;
  }
}
