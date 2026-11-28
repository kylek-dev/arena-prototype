//! `POST /api/telemetry`: scrubbed browser reports (ADR 0054, Phase 3).
//!
//! The game page sends a few small JSON reports per visit: uncaught errors,
//! unhandled rejections, WASM panics, WebGL context loss, how long startup
//! took, and one summary per finished match. It never sends per-frame data.
//! Each report is checked against a closed schema, scrubbed again here
//! (the page scrubs first), logged as one structured line for Loki and
//! counted in `/metrics`. Nothing is stored.
//!
//! Join tickets live in URL fragments, so the page sends only its origin and
//! path, and the scrubber removes any query or fragment and any long hex run
//! (a ticket or key) that reaches a message anyway. Client addresses are
//! used for the rate limit but never logged.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use tokio::time::Instant;

use super::Door;
use crate::ingress;
use crate::metrics::Metrics;

/// Largest report body accepted.
pub(super) const MAX_REPORT: usize = 2048;
/// Longest scrubbed message or page kept.
const MAX_TEXT: usize = 300;
/// Hex runs at least this long are treated as secrets (tickets are far
/// longer; a 64-bit match id has 16 digits and is not secret, but cannot be
/// told apart, so it goes too).
const SECRET_HEX: usize = 16;

/// The report kinds, in the order of `arena_door_browser_reports_total`.
pub(super) const KINDS: [&str; 6] = [
    "error",
    "rejection",
    "wasm_panic",
    "webgl_context_lost",
    "startup",
    "match_summary",
];

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Report {
    Error {
        page: String,
        message: String,
    },
    Rejection {
        page: String,
        message: String,
    },
    WasmPanic {
        page: String,
        message: String,
    },
    WebglContextLost {
        page: String,
    },
    Startup {
        page: String,
        /// From navigation to the engine's first state report.
        first_state_ms: u32,
    },
    MatchSummary {
        page: String,
        spectator: bool,
        /// Seconds from joining to the result.
        seconds: u32,
        rtt_p95_ms: Option<u32>,
        corrections: u32,
        reconnects: u32,
        hash_mismatches: u32,
        underruns: u32,
        /// Median server ticks per second on the browser's clock.
        tick_rate: Option<f64>,
        /// Median engine frame interval, milliseconds.
        frame_ms: Option<f64>,
    },
}

impl Report {
    fn kind(&self) -> usize {
        match self {
            Report::Error { .. } => 0,
            Report::Rejection { .. } => 1,
            Report::WasmPanic { .. } => 2,
            Report::WebglContextLost { .. } => 3,
            Report::Startup { .. } => 4,
            Report::MatchSummary { .. } => 5,
        }
    }
}

/// Whitespace that ends a redacted query or fragment: the same set as the
/// page's scrubber (`web/src/domain/telemetry.ts`).
fn ends_redaction(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{b}' | '\u{c}')
}

/// Removes anything that could carry a credential from browser text, in the
/// same passes as the page: URL passwords (`//user:pass@`), everything from
/// a `?` or `#` to the next whitespace, and runs of [`SECRET_HEX`] or more
/// hex digits. Control characters become spaces and the result is cut to
/// [`MAX_TEXT`] characters. `tests/fixtures/scrub-cases.json` holds cases
/// both scrubbers must agree on.
pub fn scrub(text: &str) -> String {
    // 1. `//userinfo@` (a password in a URL).
    let mut pass = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("//") {
        pass.push_str(&rest[..at + 2]);
        rest = &rest[at + 2..];
        let end = rest
            .find(|c: char| c == '/' || c == '@' || ends_redaction(c))
            .unwrap_or(rest.len());
        if rest[end..].starts_with('@') {
            pass.push_str("<redacted>");
            rest = &rest[end..];
        }
    }
    pass.push_str(rest);
    // 2. Queries and fragments.
    let mut redacted = String::with_capacity(pass.len());
    let mut chars = pass.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '?' || c == '#' {
            while chars.peek().is_some_and(|next| !ends_redaction(*next)) {
                chars.next();
            }
            redacted.push_str("<redacted>");
        } else {
            redacted.push(c);
        }
    }
    // 3. Long hex runs, and 4. control characters.
    let mut out = String::with_capacity(redacted.len());
    let mut hex = String::new();
    let flush = |hex: &mut String, out: &mut String| {
        if hex.len() >= SECRET_HEX {
            out.push_str("<hex>");
        } else {
            out.push_str(hex);
        }
        hex.clear();
    };
    for c in redacted.chars() {
        if c.is_ascii_hexdigit() {
            hex.push(c);
            continue;
        }
        flush(&mut hex, &mut out);
        out.push(if c.is_control() { ' ' } else { c });
    }
    flush(&mut hex, &mut out);
    match out.char_indices().nth(MAX_TEXT) {
        Some((cut, _)) => {
            out.truncate(cut);
            out.push('…');
            out
        }
        None => out,
    }
}

/// A finite number in `0..=max`, as the dashboards expect.
fn bounded(value: Option<f64>, max: f64) -> Result<Option<f64>, ()> {
    match value {
        Some(number) if !(number.is_finite() && (0.0..=max).contains(&number)) => Err(()),
        other => Ok(other),
    }
}

pub(super) async fn receive(
    State(door): State<Arc<Door>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Result<Bytes, axum::extract::rejection::BytesRejection>,
) -> Response {
    // Origin and rate first, so refused requests cost no parsing.
    if !door.config.origins.allows(&headers) {
        Metrics::add(&door.metrics.browser_refused[0], 1);
        return (StatusCode::FORBIDDEN, "origin not allowed").into_response();
    }
    let address = ingress::client_address(peer, &headers, &door.config.trusted_proxies);
    if door.reports.take(address, Instant::now()).is_err() {
        Metrics::add(&door.metrics.browser_refused[1], 1);
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    let report = match body.map(|body| serde_json::from_slice::<Report>(&body)) {
        Ok(Ok(report)) => report,
        // Too large (413) or unreadable.
        Err(rejection) => {
            Metrics::add(&door.metrics.browser_rejected, 1);
            return rejection.status().into_response();
        }
        Ok(Err(_)) => {
            Metrics::add(&door.metrics.browser_rejected, 1);
            return StatusCode::UNPROCESSABLE_ENTITY.into_response();
        }
    };
    let kind = report.kind();
    let name = KINDS[kind];
    match report {
        Report::Error { page, message }
        | Report::Rejection { page, message }
        | Report::WasmPanic { page, message } => {
            // Info, not warn: these are client claims, and anyone can send
            // them within the rate limit.
            tracing::info!(
                target: "arena_door::browser",
                kind = name,
                page = %scrub(&page),
                reason = %scrub(&message),
                "browser reported an error"
            );
        }
        Report::WebglContextLost { page } => {
            tracing::info!(
                target: "arena_door::browser",
                kind = name,
                page = %scrub(&page),
                "browser lost its WebGL context"
            );
        }
        Report::Startup {
            page,
            first_state_ms,
        } => {
            tracing::info!(
                target: "arena_door::browser",
                kind = name,
                page = %scrub(&page),
                first_state_ms,
                "browser started the engine"
            );
        }
        Report::MatchSummary {
            page,
            spectator,
            seconds,
            rtt_p95_ms,
            corrections,
            reconnects,
            hash_mismatches,
            underruns,
            tick_rate,
            frame_ms,
        } => {
            let (Ok(tick_rate), Ok(frame_ms)) =
                (bounded(tick_rate, 240.0), bounded(frame_ms, 10_000.0))
            else {
                Metrics::add(&door.metrics.browser_rejected, 1);
                return StatusCode::UNPROCESSABLE_ENTITY.into_response();
            };
            tracing::info!(
                target: "arena_door::browser",
                kind = name,
                page = %scrub(&page),
                spectator,
                seconds,
                rtt_p95_ms,
                corrections,
                reconnects,
                hash_mismatches,
                underruns,
                tick_rate,
                frame_ms,
                "browser finished a match"
            );
        }
    }
    Metrics::add(&door.metrics.browser_reports[kind], 1);
    StatusCode::NO_CONTENT.into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrubbing_agrees_with_the_page_on_the_shared_cases() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../../../tests/fixtures/scrub-cases.json"))
                .unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert!(cases.len() >= 10);
        for case in cases {
            let input = case["in"].as_str().unwrap();
            assert_eq!(scrub(input), case["out"].as_str().unwrap(), "{input:?}");
        }
    }

    #[test]
    fn scrubbing_removes_queries_fragments_and_long_hex() {
        let ticket = "a1".repeat(40);
        assert_eq!(
            scrub(&format!(
                "failed at https://arena.example.com/#server=x&ticket={ticket} now"
            )),
            "failed at https://arena.example.com/<redacted> now"
        );
        assert_eq!(
            scrub("GET https://arena.example.com/play?ticket=00ff reset"),
            "GET https://arena.example.com/play<redacted> reset"
        );
        assert_eq!(scrub(&format!("key {ticket}")), "key <hex>");
        // Short hex (line numbers, small values) survives.
        assert_eq!(scrub("line 42 col 7 code ff"), "line 42 col 7 code ff");
        assert_eq!(scrub("match 0123456789abcdef"), "match <hex>");
        assert_eq!(scrub("a\nb\tc"), "a b c");
    }

    #[test]
    fn scrubbed_text_is_bounded_on_a_character_boundary() {
        let long = "é".repeat(1000);
        let scrubbed = scrub(&long);
        assert_eq!(scrubbed.chars().count(), MAX_TEXT + 1);
        assert!(scrubbed.ends_with('…'));
    }

    #[test]
    fn reports_follow_a_closed_schema() {
        let parse = |json: &str| serde_json::from_str::<Report>(json);
        assert!(parse(r#"{"kind":"error","page":"/","message":"x"}"#).is_ok());
        assert!(parse(r#"{"kind":"webgl_context_lost","page":"/"}"#).is_ok());
        // Unknown kinds and unknown fields are refused.
        assert!(parse(r#"{"kind":"keystrokes","page":"/"}"#).is_err());
        assert!(parse(r#"{"kind":"error","page":"/","message":"x","ticket":"00"}"#).is_err());
        // Counts are unsigned and bounded by their type.
        assert!(parse(r#"{"kind":"startup","page":"/","first_state_ms":-1}"#).is_err());
        assert!(parse(r#"{"kind":"startup","page":"/","first_state_ms":1e12}"#).is_err());
        assert_eq!(bounded(Some(f64::NAN), 240.0), Err(()));
        assert_eq!(bounded(Some(241.0), 240.0), Err(()));
        assert_eq!(bounded(Some(29.9), 240.0), Ok(Some(29.9)));
        assert_eq!(bounded(None, 240.0), Ok(None));
    }
}
