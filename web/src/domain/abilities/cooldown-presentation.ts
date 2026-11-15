export type CooldownSample = { tick: number; remaining: number; duration: number };
export type CooldownVisual = { active: boolean; sweep: number; label: string };

// Presentation only: callers supply timestamps explicitly. This never changes
// an authoritative sample or decides whether an ability can be activated.
export class CooldownTimeline {
  private sample: CooldownSample | null = null;
  private anchorRemaining = 0;
  private anchorAt = 0;
  private lastObservedAt = 0;

  observe(sample: CooldownSample | null, now: number) {
    const previous = this.sample;
    const restart =
      !sample ||
      !previous ||
      sample.tick < previous.tick ||
      sample.remaining > previous.remaining ||
      sample.duration !== previous.duration ||
      now - this.lastObservedAt > 250;

    if (restart || sample?.remaining === 0) {
      this.anchorRemaining = sample?.remaining ?? 0;
      this.anchorAt = now;
    } else if (
      sample &&
      (sample.tick !== previous?.tick || sample.remaining !== previous?.remaining)
    ) {
      // Tick quantization must not make a running sweep jump backwards. A real
      // cooldown increase/reset above is allowed to restart it immediately.
      this.anchorRemaining = Math.min(sample.remaining, this.estimate(now));
      this.anchorAt = now;
    }

    this.sample = sample;
    this.lastObservedAt = now;
  }

  private estimate(now: number) {
    // Do not count down indefinitely when simulation snapshots stop arriving.
    const elapsed = Math.min(150, Math.max(0, now - this.anchorAt)) / 1000;
    return Math.max(0, this.anchorRemaining - elapsed);
  }

  visual(now: number): CooldownVisual {
    if (!this.sample || this.sample.remaining <= 0) {
      return { active: false, sweep: 360, label: "" };
    }
    const remaining = Math.max(0.001, this.estimate(now));
    const fraction = Math.min(1, remaining / this.sample.duration);
    return {
      active: true,
      // Reveal clockwise from twelve o'clock; leave a sliver until Rust says 0.
      sweep: (1 - fraction) * 360,
      label: Math.max(0.1, remaining).toFixed(1),
    };
  }
}
