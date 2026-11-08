export const SPIKE_MS = 1000 / 30;
const GRAPH_BUCKET_MS = 50;
const GRAPH_BUCKETS = 120;

/** Bounded browser-frame history, independent of simulation ticks and rendering. */
export class FrameTiming {
  private durations = new Float64Array(3600);
  private timestamps = new Float64Array(3600);
  private cursor = 0;
  private count = 0;

  reset() {
    this.cursor = this.count = 0;
  }

  add(now: number, duration: number) {
    if (!Number.isFinite(now) || !Number.isFinite(duration) || duration <= 0) return;
    this.durations[this.cursor] = duration;
    this.timestamps[this.cursor] = now;
    this.cursor = (this.cursor + 1) % this.durations.length;
    this.count = Math.min(this.count + 1, this.durations.length);
  }

  summary(now: number) {
    const sorted = Array.from(this.durations.subarray(0, this.count)).sort((a, b) => a - b);
    const percentile = (p: number) => sorted[Math.ceil(sorted.length * p) - 1] ?? null;
    const graph = Array<number>(GRAPH_BUCKETS).fill(0);
    let recentMs = 0;
    let recentCount = 0;
    let spikes = 0;
    for (let i = 0; i < this.count; i++) {
      const duration = this.durations[i];
      const age = now - this.timestamps[i];
      if (duration > SPIKE_MS) spikes++;
      if (age >= 0 && age < 1000) {
        recentMs += duration;
        recentCount++;
      }
      const bucket = GRAPH_BUCKETS - 1 - Math.floor(age / GRAPH_BUCKET_MS);
      if (age >= 0 && bucket >= 0 && bucket < GRAPH_BUCKETS)
        graph[bucket] = Math.max(graph[bucket], duration);
    }
    return {
      fps: recentCount ? (1000 * recentCount) / recentMs : null,
      frameSamples: this.count,
      frameP50Ms: percentile(0.5),
      frameP95Ms: percentile(0.95),
      frameP99Ms: percentile(0.99),
      frameMaxMs: sorted.at(-1) ?? null,
      frameSpikes: spikes,
      graph,
    };
  }
}
