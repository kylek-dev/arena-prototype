import { describe, expect, it } from "vitest";
import {
  MAX_TEXT,
  MatchSummaryCollector,
  REPORT_BUDGET,
  ReportBudget,
  Reservoir,
  SUMMARY_BUDGET,
  errorKind,
  scrubPage,
  scrubText,
  telemetryChoice,
  telemetryRefused,
} from "../../web/src/domain/telemetry";
import type { LinkState, NetworkStatus } from "../../web/src/domain/network";
import { createReporter, installTelemetry } from "../../web/src/runtime/telemetry";
import fixture from "../fixtures/scrub-cases.json";

const TICKET = "a1b2".repeat(20);
const JOIN = `https://arena.example.com/game/?debug=1#server=wss%3A%2F%2Farena.example.com%2Fplay&ticket=${TICKET}&expires=1`;

describe("scrubbing", () => {
  it("reports the page without its query or fragment", () => {
    expect(scrubPage(JOIN)).toBe("https://arena.example.com/game/");
    expect(scrubPage("not a url")).toBe("");
  });

  it("agrees with the door on every shared case", () => {
    expect(fixture.cases.length).toBeGreaterThanOrEqual(10);
    for (const { in: input, out } of fixture.cases) expect(scrubText(input), input).toBe(out);
  });

  it("removes queries, fragments and long hex from messages", () => {
    expect(scrubText(`failed to load ${JOIN} now`)).toBe(
      "failed to load https://arena.example.com/game/<redacted> now",
    );
    expect(scrubText(`bad ticket ${TICKET}`)).toBe("bad ticket <hex>");
    expect(scrubText("line 42 code ff")).toBe("line 42 code ff");
    expect(scrubText("a\nb")).toBe("a b");
    const long = scrubText("é".repeat(1000));
    expect(Array.from(long)).toHaveLength(MAX_TEXT + 1);
  });

  it("never lets a ticket through any report", () => {
    const sent: string[] = [];
    const reporter = createReporter({ send: (body) => sent.push(body), page: () => JOIN });
    reporter.error(new Error(TICKET), `Uncaught ${JOIN}`);
    reporter.rejection(new Error(`fetch ${JOIN} failed`));
    reporter.contextLost();
    reporter.startup(1234.4);
    expect(sent).toHaveLength(4);
    for (const body of sent) {
      expect(body).not.toContain(TICKET);
      expect(body).not.toContain("ticket=");
      expect(body).not.toContain("#server");
      expect(JSON.parse(body).page).toBe("https://arena.example.com/game/");
    }
    expect(JSON.parse(sent[3])).toEqual({
      kind: "startup",
      first_state_ms: 1234,
      page: "https://arena.example.com/game/",
    });
  });
});

describe("limits and consent", () => {
  it("sends each message once and stops at the budget", () => {
    const budget = new ReportBudget();
    const report = (message: string) => ({ kind: "error" as const, page: "/", message });
    expect(budget.allow(report("same"))).toBe(true);
    expect(budget.allow(report("same"))).toBe(false);
    let allowed = 1;
    for (let i = 0; i < 100; i++) if (budget.allow(report(`m${i}`))) allowed++;
    expect(allowed).toBe(REPORT_BUDGET);
  });

  it("respects Do Not Track, Global Privacy Control and ?telemetry=off", () => {
    expect(telemetryRefused({}, "")).toBe(false);
    expect(telemetryRefused({ doNotTrack: "1" }, "")).toBe(true);
    expect(telemetryRefused({ globalPrivacyControl: true }, "")).toBe(true);
    expect(telemetryRefused({}, "?debug=1&telemetry=off")).toBe(true);
    // A remembered choice holds until the page says otherwise.
    expect(telemetryRefused({}, "", "off")).toBe(true);
    expect(telemetryRefused({}, "?telemetry=on", "off")).toBe(false);
    expect(telemetryChoice("?telemetry=maybe")).toBeNull();
  });

  it("keeps match summaries out of the error budget", () => {
    const budget = new ReportBudget();
    for (let i = 0; i < REPORT_BUDGET; i++)
      expect(budget.allow({ kind: "error", page: "/", message: `m${i}` })).toBe(true);
    const summary = {
      kind: "match_summary" as const,
      page: "/",
      spectator: false,
      seconds: 1,
      rtt_p95_ms: null,
      corrections: 0,
      reconnects: 0,
      hash_mismatches: 0,
      underruns: 0,
      tick_rate: null,
      frame_ms: null,
    };
    let summaries = 0;
    for (let i = 0; i < SUMMARY_BUDGET + 5; i++) if (budget.allow(summary)) summaries++;
    expect(summaries).toBe(SUMMARY_BUDGET);
    expect(budget.allow({ kind: "error", page: "/", message: "late" })).toBe(false);
  });

  it("classifies engine panics", () => {
    const panic = new Error("unreachable");
    panic.name = "RuntimeError";
    expect(errorKind(panic, "RuntimeError: unreachable")).toBe("wasm_panic");
    expect(errorKind(null, "Uncaught RuntimeError: unreachable executed")).toBe("wasm_panic");
    expect(errorKind(new TypeError("x"), "TypeError: x")).toBe("error");
  });
});

describe("the page runtime", () => {
  it("sends nothing after ?telemetry=off, on this and later visits", () => {
    const sent: string[] = [];
    window.history.replaceState(null, "", "/?telemetry=off");
    const off = installTelemetry(document.createElement("canvas"), (body) => sent.push(body));
    off.failure(new Error("x"), "x");
    window.dispatchEvent(new ErrorEvent("error", { message: "boom" }));
    off.stop();
    window.history.replaceState(null, "", "/");
    const later = installTelemetry(document.createElement("canvas"), (body) => sent.push(body));
    later.failure(new Error("y"), "y");
    later.stop();
    expect(sent).toEqual([]);
    window.history.replaceState(null, "", "/?telemetry=on");
    const on = installTelemetry(document.createElement("canvas"), (body) => sent.push(body));
    on.stop();
    window.history.replaceState(null, "", "/");
  });

  it("reports caught failures, uncaught errors and context loss, scrubbed", () => {
    const sent: string[] = [];
    window.history.replaceState(null, "", `/game/#server=x&ticket=${TICKET}`);
    const canvas = document.createElement("canvas");
    const telemetry = installTelemetry(canvas, (body) => sent.push(body));
    telemetry.failure(new Error("engine"), `Failed to load pkg/arena_client.js?v=${TICKET}`);
    window.dispatchEvent(
      new ErrorEvent("error", { message: "Uncaught RuntimeError: unreachable" }),
    );
    canvas.dispatchEvent(new Event("webglcontextlost"));
    telemetry.stop();
    window.dispatchEvent(new ErrorEvent("error", { message: "after stop" }));
    window.history.replaceState(null, "", "/");
    const kinds = sent.map((body) => JSON.parse(body).kind);
    expect(kinds).toEqual(["error", "wasm_panic", "webgl_context_lost"]);
    for (const body of sent) expect(body).not.toContain(TICKET);
    expect(JSON.parse(sent[0]).message).toBe("Failed to load pkg/arena_client.js<redacted>");
    expect(JSON.parse(sent[0]).page).toBe(`${location.origin}/game/`);
  });
});

describe("match summaries", () => {
  const status = (patch: Partial<NetworkStatus>): NetworkStatus => ({
    phase: "playing",
    error: null,
    seat: 0,
    mapDiffers: false,
    rttMs: 40,
    inputMs: null,
    delayMs: 0,
    snapshots: 0,
    underruns: 0,
    hashMismatches: 0,
    predictedTicks: 0,
    corrections: 0,
    lastCorrectionMm: null,
    queued: null,
    queuedMin: null,
    pace: 1,
    skippedInputs: 0,
    stepsPerSecond: 30,
    sentPerSecond: 30,
    tickRate: 30,
    ...patch,
  });
  const link = (phase: LinkState["phase"], s: NetworkStatus | null): LinkState => ({
    phase,
    attempt: 0,
    ended: null,
    status: s,
  });
  const frame = (ms: number | null) => () => ms;

  it("yields one summary per finished match, with p95 RTT and reconnects", () => {
    const collector = new MatchSummaryCollector();
    expect(
      collector.observe(link("open", status({ phase: "connecting" })), 0, frame(null)),
    ).toBeNull();
    for (let i = 0; i < 20; i++)
      expect(
        collector.observe(
          link("open", status({ rttMs: 40 + i, corrections: i })),
          i * 1000,
          frame(16.6),
        ),
      ).toBeNull();
    collector.observe(link("reconnecting", null), 21_000, frame(16.6));
    collector.observe(link("reconnecting", null), 21_500, frame(16.6));
    const summary = collector.observe(
      link("open", status({ phase: "finished", underruns: 2 })),
      62_000,
      frame(16.66),
    );
    expect(summary).toEqual({
      spectator: false,
      seconds: 62,
      rtt_p95_ms: 58,
      corrections: 19,
      reconnects: 1,
      hash_mismatches: 0,
      underruns: 2,
      tick_rate: 30,
      frame_ms: 16.7,
    });
    // The finished status keeps arriving: no second summary.
    expect(
      collector.observe(link("open", status({ phase: "finished" })), 63_000, frame(16)),
    ).toBeNull();
    // The next match starts afresh.
    collector.observe(link("open", status({ phase: "waiting", rttMs: 10 })), 70_000, frame(null));
    const next = collector.observe(
      link("ended", status({ phase: "closed", rttMs: 10 })),
      75_000,
      frame(null),
    );
    expect(next).toMatchObject({ seconds: 5, rtt_p95_ms: 10, reconnects: 0, corrections: 0 });
  });

  it("never summarizes a match that never started, nor counts its reconnects", () => {
    const collector = new MatchSummaryCollector();
    collector.observe(link("reconnecting", null), 0, frame(null));
    expect(collector.observe(link("ended", null), 1, frame(null))).toBeNull();
    collector.observe(link("open", status({ phase: "playing" })), 2_000, frame(null));
    expect(
      collector.observe(link("open", status({ phase: "finished" })), 3_000, frame(null)),
    ).toMatchObject({
      reconnects: 0,
    });
  });

  it("marks spectators and reads the frame time only when finishing", () => {
    const collector = new MatchSummaryCollector();
    let reads = 0;
    const counted = () => {
      reads++;
      return 20;
    };
    for (let i = 0; i < 100; i++)
      collector.observe(link("open", status({ spectator: true, seat: null })), i, counted);
    expect(reads).toBe(0);
    const summary = collector.observe(
      link("open", status({ phase: "finished", spectator: true, seat: null })),
      100,
      counted,
    );
    expect(reads).toBe(1);
    expect(summary).toMatchObject({ spectator: true, frame_ms: 20 });
  });

  it("samples a whole long match uniformly within a fixed size", () => {
    let seed = 7;
    const random = () => {
      seed = (seed * 16807) % 2147483647;
      return seed / 2147483647;
    };
    const reservoir = new Reservoir(MatchSummaryCollector.MAX_SAMPLES, random);
    for (let i = 0; i < 20_000; i++) reservoir.add(i < 10_000 ? 10 : 200);
    expect(reservoir.values).toHaveLength(MatchSummaryCollector.MAX_SAMPLES);
    const late = reservoir.values.filter((v) => v === 200).length / reservoir.values.length;
    // Half the stream came late: about half the sample does too.
    expect(late).toBeGreaterThan(0.4);
    expect(late).toBeLessThan(0.6);
  });
});
