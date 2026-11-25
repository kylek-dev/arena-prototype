// Sends scrubbed browser reports to the front door (ADR 0054, Phase 3):
// uncaught and caught failures (errors, rejections, WASM panics), WebGL
// context loss, the time to the engine's first state, and one summary per
// finished match. Same origin only (`/api/telemetry`, which the reverse proxy
// and the Vite dev proxy route to `arena-door`). Off when the browser sends
// Do Not Track or Global Privacy Control, or after `?telemetry=off` (kept on
// this device until `?telemetry=on`).
import {
  MatchSummaryCollector,
  type Report,
  ReportBudget,
  errorKind,
  scrubPage,
  scrubText,
  telemetryChoice,
  telemetryRefused,
} from "../domain/telemetry";
import { networkLink } from "../state/network";
import { firstStateSinceNavigation, metrics } from "./diagnostics";

const OPT_OUT_KEY = "arena.telemetry";
/** Stop waiting for the engine's first state after this long. */
const STARTUP_DEADLINE_MS = 5 * 60_000;

export type TelemetryDeps = {
  send: (body: string) => void;
  page: () => string;
};

function beacon(body: string) {
  const url = new URL("/api/telemetry", location.origin).href;
  const blob = new Blob([body], { type: "application/json" });
  if (navigator.sendBeacon?.(url, blob)) return;
  void fetch(url, { method: "POST", body: blob, keepalive: true }).catch(() => {});
}

/** A report before its page is added (distributes over the kinds). */
type Unpaged<R> = R extends unknown ? Omit<R, "page"> : never;

/** The reporter, separate from the browser so tests can drive it. */
export function createReporter(deps: TelemetryDeps) {
  const budget = new ReportBudget();
  const report = (partial: Unpaged<Report>) => {
    const full = {
      ...partial,
      page: scrubPage(deps.page()),
      ...("message" in partial ? { message: scrubText(partial.message) } : {}),
    } as Report;
    if (budget.allow(full)) deps.send(JSON.stringify(full));
  };
  return {
    error(error: unknown, message: string) {
      report({ kind: errorKind(error, message), message: message || "error" });
    },
    rejection(reason: unknown) {
      const message = String((reason as { message?: unknown })?.message ?? reason);
      report({ kind: errorKind(reason, message), message: message || "rejected" });
    },
    contextLost() {
      report({ kind: "webgl_context_lost" });
    },
    startup(firstStateMs: number) {
      report({ kind: "startup", first_state_ms: Math.max(0, Math.round(firstStateMs)) });
    },
    summary(summary: Omit<Extract<Report, { kind: "match_summary" }>, "kind" | "page">) {
      report({ kind: "match_summary", ...summary });
    },
  };
}

/** The remembered `?telemetry=` choice, updated from this page's address. */
function rememberedChoice(search: string): string | null {
  try {
    const choice = telemetryChoice(search);
    if (choice === "off") localStorage.setItem(OPT_OUT_KEY, "off");
    if (choice === "on") localStorage.removeItem(OPT_OUT_KEY);
    return localStorage.getItem(OPT_OUT_KEY);
  } catch {
    return null;
  }
}

export type Telemetry = {
  /** Reports a failure the page caught itself (it shows its own error). */
  failure(error: unknown, message: string): void;
  stop(): void;
};

/** Starts reporting for this page. `send` replaces the beacon in tests. */
export function installTelemetry(
  canvas: HTMLCanvasElement,
  send: (body: string) => void = beacon,
): Telemetry {
  const nav = navigator as Navigator & { globalPrivacyControl?: boolean };
  if (telemetryRefused(nav, location.search, rememberedChoice(location.search)))
    return { failure() {}, stop() {} };
  const reporter = createReporter({ send, page: () => location.href });
  const onError = (event: ErrorEvent) => reporter.error(event.error, event.message);
  const onRejection = (event: PromiseRejectionEvent) => reporter.rejection(event.reason);
  const onContextLost = () => reporter.contextLost();
  window.addEventListener("error", onError);
  window.addEventListener("unhandledrejection", onRejection);
  canvas.addEventListener("webglcontextlost", onContextLost);
  // Startup: once, when the engine's first state has arrived, measured from
  // navigation. A page that never starts is reported through `failure`.
  const startupBegan = performance.now();
  const startupTimer = window.setInterval(() => {
    const first = firstStateSinceNavigation();
    if (first !== null) reporter.startup(first);
    if (first !== null || performance.now() - startupBegan > STARTUP_DEADLINE_MS)
      window.clearInterval(startupTimer);
  }, 1000);
  // One summary per finished match, from the link's state reports.
  const collector = new MatchSummaryCollector();
  const frameMs = () => metrics().engine.frameP50Ms;
  let unsubscribe: (() => void) | null = null;
  const watch = () => {
    unsubscribe?.();
    const link = networkLink.get();
    unsubscribe =
      link?.state.subscribe(() => {
        const summary = collector.observe(link.state.get(), performance.now(), frameMs);
        if (summary) reporter.summary(summary);
      }) ?? null;
  };
  watch();
  const stopWatchingLink = networkLink.subscribe(watch);
  return {
    failure: (error, message) => reporter.error(error, message),
    stop() {
      window.removeEventListener("error", onError);
      window.removeEventListener("unhandledrejection", onRejection);
      canvas.removeEventListener("webglcontextlost", onContextLost);
      window.clearInterval(startupTimer);
      stopWatchingLink();
      unsubscribe?.();
    },
  };
}
