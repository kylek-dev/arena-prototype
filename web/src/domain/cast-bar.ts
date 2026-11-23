// Pure cast-bar presentation (no DOM, React or engine dependencies).
// Callers pass timestamps in milliseconds explicitly.
import type { ActiveCast } from "./casting";

export type CastSample = { cast: ActiveCast; tickRate: number };
/** `settled`: the bar cannot move again until a new sample arrives (full, or stalled). */
export type CastVisual = { active: boolean; progress: number; remaining: number; settled: boolean };

/** Most a running bar may drift from a fresh authoritative sample before it snaps. */
export const CAST_RESYNC_SECONDS = 0.2;
/** How far a bar may run past the last authoritative sample when snapshots stall. */
export const CAST_STALL_SECONDS = 0.25;

const IDLE: CastVisual = { active: false, progress: 0, remaining: 0, settled: true };

/**
 * Runs a cast bar from the cast's progress and duration between throttled
 * snapshots. It snaps only when a cast starts (a new spell, a new duration or
 * remaining time going back up), ends, or falls more than
 * {@link CAST_RESYNC_SECONDS} behind or ahead of a fresh sample. A sample
 * ahead of the bar pulls it forward; small jitter behind it never moves the
 * bar backwards. This never changes authoritative state.
 */
export class CastTimeline {
  private cast: ActiveCast | null = null;
  private rate = 0;
  private duration = 0;
  private anchorElapsed = 0;
  private anchorAt = 0;
  private sampleElapsed = 0;

  observe(sample: CastSample | null, now: number) {
    if (!sample || !Number.isFinite(sample.tickRate) || sample.tickRate <= 0) {
      this.cast = null;
      return;
    }
    const { cast, tickRate } = sample;
    const elapsed = Math.max(0, cast.totalTicks - cast.remainingTicks) / tickRate;
    const previous = this.cast;
    const restart =
      !previous ||
      previous.spell !== cast.spell ||
      previous.totalTicks !== cast.totalTicks ||
      cast.remainingTicks > previous.remainingTicks ||
      tickRate !== this.rate;
    this.cast = cast;
    this.rate = tickRate;
    if (restart) {
      this.duration = cast.totalTicks / tickRate;
      this.sampleElapsed = elapsed;
      this.anchorElapsed = elapsed;
      this.anchorAt = now;
      return;
    }
    // A repeated sample (a re-render, a stalled feed) carries no new timing.
    if (cast.remainingTicks === previous.remainingTicks) return;
    this.sampleElapsed = elapsed;
    const estimate = this.estimate(now);
    if (elapsed > estimate || estimate - elapsed > CAST_RESYNC_SECONDS) {
      this.anchorElapsed = elapsed;
      this.anchorAt = now;
    }
  }

  private running(now: number) {
    return this.anchorElapsed + Math.max(0, now - this.anchorAt) / 1000;
  }

  private limit() {
    return Math.min(this.sampleElapsed + CAST_STALL_SECONDS, this.duration);
  }

  private estimate(now: number) {
    return Math.min(this.running(now), this.limit());
  }

  visual(now: number): CastVisual {
    if (!this.cast || this.duration <= 0) return IDLE;
    const elapsed = this.estimate(now);
    return {
      active: true,
      progress: Math.min(1, Math.max(0, elapsed / this.duration)),
      remaining: Math.max(0, this.duration - elapsed),
      settled: this.running(now) >= this.limit(),
    };
  }
}
