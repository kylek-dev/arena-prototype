// Scrubbed browser telemetry (ADR 0054, Phase 3): what the page reports to
// the front door's `/api/telemetry`, and the scrubbing that keeps join
// tickets and keys out of it. Pure, so the rules are unit-tested; the
// runtime in `runtime/telemetry.ts` gathers the facts and sends them.
//
// Never per frame: a visit sends at most `REPORT_BUDGET` error reports and
// one summary per finished match.
import type { LinkState, NetworkStatus } from "./network";

/** Error reports (errors, rejections, panics, context loss, startup) one
 * page load may send. */
export const REPORT_BUDGET = 20;
/** Match summaries one page load may send: one per finished match, so a
 * long queue session keeps reporting; the door's rate limit bounds volume. */
export const SUMMARY_BUDGET = 200;
/** Longest message kept, in characters. */
export const MAX_TEXT = 300;
/** Hex runs this long or longer may be a ticket or key and are removed. */
export const SECRET_HEX = 16;

export type Report =
  | { kind: "error" | "rejection" | "wasm_panic"; page: string; message: string }
  | { kind: "webgl_context_lost"; page: string }
  | { kind: "startup"; page: string; first_state_ms: number }
  | ({ kind: "match_summary"; page: string } & MatchSummary);

export type MatchSummary = {
  spectator: boolean;
  seconds: number;
  rtt_p95_ms: number | null;
  corrections: number;
  reconnects: number;
  hash_mismatches: number;
  underruns: number;
  tick_rate: number | null;
  frame_ms: number | null;
};

/** The page as reported: origin and path only, never the query or the
 * fragment (where join links carry tickets). */
export function scrubPage(href: string): string {
  try {
    const url = new URL(href);
    return scrubText(`${url.origin}${url.pathname}`);
  } catch {
    return "";
  }
}

/** Whitespace that ends a redacted query or fragment: the same set the
 * door uses (`crates/arena-server/src/door/browser.rs`). */
const WHITESPACE = " \\t\\n\\r\\u000b\\u000c";

/** Removes anything that could carry a credential from free text: URL
 * passwords (`//user:pass@`), everything from a `?` or `#` to the next
 * whitespace, and long hex runs. Control characters become spaces; the
 * result is cut to `MAX_TEXT` characters. The front door applies the same
 * rules again; `tests/fixtures/scrub-cases.json` holds cases both pass. */
export function scrubText(text: string): string {
  const out = text
    .replace(new RegExp(`//[^/${WHITESPACE}@]*@`, "g"), "//<redacted>@")
    .replace(new RegExp(`[?#][^${WHITESPACE}]*`, "g"), "<redacted>")
    .replace(new RegExp(`[0-9a-fA-F]{${SECRET_HEX},}`, "g"), "<hex>")
    .replace(/\p{Cc}/gu, " ");
  const chars = Array.from(out);
  return chars.length > MAX_TEXT ? `${chars.slice(0, MAX_TEXT).join("")}…` : out;
}

/** Which kind an uncaught error is: Rust panics in the engine surface as
 * WebAssembly `RuntimeError`s (`unreachable`). */
export function errorKind(error: unknown, message: string): "error" | "wasm_panic" {
  const name = error instanceof Error ? error.name : "";
  return name === "RuntimeError" || /\bunreachable\b/i.test(message) ? "wasm_panic" : "error";
}

/** `?telemetry=off` or `?telemetry=on` in the address: a choice to remember
 * on this device; `null` when the page names none. */
export function telemetryChoice(search: string): "off" | "on" | null {
  const value = new URLSearchParams(search).get("telemetry");
  return value === "off" || value === "on" ? value : null;
}

/** Whether to send nothing: the browser asked not to be tracked (Do Not
 * Track or Global Privacy Control), or the player turned reports off with
 * `?telemetry=off`, now or on an earlier visit (`stored`). */
export function telemetryRefused(
  nav: { doNotTrack?: string | null; globalPrivacyControl?: boolean },
  search: string,
  stored: string | null = null,
) {
  const choice = telemetryChoice(search) ?? stored;
  return nav.doNotTrack === "1" || nav.globalPrivacyControl === true || choice === "off";
}

/** At most `REPORT_BUDGET` error reports per page load, each distinct
 * message once, and separately at most `SUMMARY_BUDGET` match summaries. */
export class ReportBudget {
  private reports = 0;
  private summaries = 0;
  private seen = new Set<string>();
  constructor(
    private readonly budget = REPORT_BUDGET,
    private readonly summaryBudget = SUMMARY_BUDGET,
  ) {}

  allow(report: Report): boolean {
    if (report.kind === "match_summary") {
      if (this.summaries >= this.summaryBudget) return false;
      this.summaries++;
      return true;
    }
    if (this.reports >= this.budget) return false;
    if ("message" in report) {
      const key = `${report.kind}:${report.message}`;
      if (this.seen.has(key)) return false;
      this.seen.add(key);
    }
    this.reports++;
    return true;
  }
}

/** A uniform sample of at most `capacity` values from a stream of any length
 * (reservoir sampling), so a long match is not judged by its first minutes. */
export class Reservoir {
  readonly values: number[] = [];
  private seen = 0;
  constructor(
    private readonly capacity: number,
    private readonly random: () => number = Math.random,
  ) {}

  add(value: number) {
    this.seen++;
    if (this.values.length < this.capacity) {
      this.values.push(value);
      return;
    }
    const slot = Math.floor(this.random() * this.seen);
    if (slot < this.capacity) this.values[slot] = value;
  }
}

/** The value at quantile `q` of `values`, or null when empty. */
function quantile(values: number[], q: number): number | null {
  if (!values.length) return null;
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.min(sorted.length - 1, Math.ceil(sorted.length * q) - 1)];
}

/** Gathers one match's network health from the link's state reports and
 * yields a single summary when it finishes. Samples are bounded and uniform
 * over the whole match. A rejoin after the link ended starts a new summary
 * (the first part was summarized when the link ended). */
export class MatchSummaryCollector {
  static readonly MAX_SAMPLES = 2000;
  private active = false;
  private startedMs = 0;
  private spectator = false;
  private rtt: Reservoir;
  private tickRates: Reservoir;
  private corrections = 0;
  private hashMismatches = 0;
  private underruns = 0;
  private reconnects = 0;
  private lastPhase: LinkState["phase"] = "idle";

  constructor(private readonly random: () => number = Math.random) {
    this.rtt = new Reservoir(MatchSummaryCollector.MAX_SAMPLES, random);
    this.tickRates = new Reservoir(MatchSummaryCollector.MAX_SAMPLES, random);
  }

  /** Feeds the link's state; returns the summary once, when the match
   * finishes (a result arrives or the link ends after playing). `frameMs` is
   * read only then: the median of the engine's recent frames. */
  observe(state: LinkState, nowMs: number, frameMs: () => number | null): MatchSummary | null {
    const status: NetworkStatus | null = state.status;
    if (state.phase === "reconnecting" && this.lastPhase !== "reconnecting" && this.active)
      this.reconnects++;
    this.lastPhase = state.phase;
    if (status && (status.phase === "waiting" || status.phase === "playing") && !this.active) {
      this.reset();
      this.active = true;
      this.startedMs = nowMs;
    }
    if (!this.active) return null;
    if (status) {
      this.spectator = status.spectator === true;
      if (status.rttMs !== null && Number.isFinite(status.rttMs)) this.rtt.add(status.rttMs);
      if (status.tickRate !== null && Number.isFinite(status.tickRate))
        this.tickRates.add(status.tickRate);
      // The engine's counters can restart with a new connection; keeping the
      // highest seen may undercount after a reconnect, never overcount.
      this.corrections = Math.max(this.corrections, status.corrections);
      this.hashMismatches = Math.max(this.hashMismatches, status.hashMismatches);
      this.underruns = Math.max(this.underruns, status.underruns);
    }
    const finished = status?.phase === "finished" || state.phase === "ended";
    if (!finished) return null;
    this.active = false;
    const round = (value: number | null) => (value === null ? null : Math.round(value));
    const frame = frameMs();
    return {
      spectator: this.spectator,
      seconds: Math.max(0, Math.round((nowMs - this.startedMs) / 1000)),
      rtt_p95_ms: round(quantile(this.rtt.values, 0.95)),
      corrections: this.corrections,
      reconnects: this.reconnects,
      hash_mismatches: this.hashMismatches,
      underruns: this.underruns,
      tick_rate: quantile(this.tickRates.values, 0.5),
      frame_ms: frame !== null && Number.isFinite(frame) ? Math.round(frame * 10) / 10 : null,
    };
  }

  private reset() {
    this.rtt = new Reservoir(MatchSummaryCollector.MAX_SAMPLES, this.random);
    this.tickRates = new Reservoir(MatchSummaryCollector.MAX_SAMPLES, this.random);
    this.corrections = this.hashMismatches = this.underruns = this.reconnects = 0;
  }
}
