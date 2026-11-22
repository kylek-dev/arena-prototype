// Authoritative round-clock formatting and result labels (no wall clock or DOM).
import type { RoundResult, Snapshot } from "../bridge/snapshot";

export type RoundClockView = {
  text: string;
  seconds: number;
  warning: boolean;
  expired: boolean;
};

export type RoundResultTone = "victory" | "defeat" | "draw" | "time-limit";

export type RoundResultBanner = {
  label: string;
  detail: string;
  tone: RoundResultTone;
};

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

/** Remaining whole seconds from an engine snapshot, or `null` when clock metadata is absent. */
export function snapshotRemainingSeconds(
  snapshot: Pick<Snapshot, "tick" | "maxRoundTicks" | "tickRate"> | null,
): number | null {
  if (
    !snapshot ||
    snapshot.maxRoundTicks === undefined ||
    snapshot.tickRate === undefined ||
    !Number.isFinite(snapshot.maxRoundTicks) ||
    !Number.isFinite(snapshot.tickRate) ||
    snapshot.maxRoundTicks <= 0 ||
    snapshot.tickRate <= 0
  ) {
    return null;
  }
  return remainingRoundSeconds(snapshot.tick, snapshot.maxRoundTicks, snapshot.tickRate);
}

/** Derives HUD clock presentation from pre-computed remaining seconds and round outcome. */
export function roundClockFromSeconds(
  seconds: number,
  won: boolean,
  roundResult: RoundResult | null | undefined,
): RoundClockView {
  const clamped = Number.isFinite(seconds) ? Math.max(0, Math.floor(seconds)) : 0;
  const expired = roundResult === "time_limit_draw" || (!won && clamped <= 0);
  return {
    text: formatRoundClock(clamped),
    seconds: clamped,
    warning: !won && !expired && isRoundClockWarning(clamped),
    expired,
  };
}

/** Derives the HUD round clock presentation from authoritative snapshot fields.
 * Returns `null` when snapshot metadata is absent (older client builds). */
export function roundClockView(
  snapshot: Pick<Snapshot, "tick" | "maxRoundTicks" | "tickRate" | "won" | "roundResult"> | null,
): RoundClockView | null {
  const seconds = snapshotRemainingSeconds(snapshot);
  if (seconds === null || !snapshot) return null;
  return roundClockFromSeconds(seconds, snapshot.won, snapshot.roundResult);
}

/** Visual tone for an authoritative round outcome. */
export function roundResultTone(result: RoundResult): RoundResultTone {
  switch (result) {
    case "won":
      return "victory";
    case "lost":
      return "defeat";
    case "time_limit_draw":
      return "time-limit";
    case "draw":
      return "draw";
  }
}

/** Derives the HUD round-result banner content and visual tone from an authoritative outcome. */
export function roundResultBanner(
  result: RoundResult | null | undefined,
  networked: boolean,
): RoundResultBanner | null {
  const label = roundResultLabel(result);
  if (!label || !result) return null;
  const tone = roundResultTone(result);
  const detail =
    result === "time_limit_draw"
      ? networked
        ? "Round time limit reached · match closing shortly"
        : "Round time limit reached"
      : networked
        ? "Match closing shortly"
        : "Round complete";
  return { label, detail, tone };
}
