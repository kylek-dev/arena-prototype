//! Process logs for `arena-server` and `arena-door` (ADR 0054).
//!
//! Every line is a `tracing` event on stderr. `ARENA_LOG_FORMAT` picks the
//! shape: `pretty` (the default) for a terminal, `json` (one object per line,
//! as the Docker deployment sets) for log shippers, matching Caddy's JSON
//! access log. `RUST_LOG` filters as usual and defaults to `info`.
//!
//! Lifecycle events carry the same field names everywhere, so one match or
//! connection can be followed from the front door to the match server:
//! `match_id` (16 hex digits, as the lobby shows it), `seat`, `conn_id`,
//! `team_size`, `mode`, `error_code` and `reason`.
//!
//! Never log a ticket, a join link or `ARENA_TICKET_KEY`: tickets are bearer
//! credentials. Nothing here logs per tick or per input; the hot path
//! reports through `/metrics` only.

use std::fmt;
use std::io::IsTerminal;

use tracing_subscriber::EnvFilter;

/// How log lines are written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogFormat {
    /// Human-readable, coloured only when stderr is a terminal.
    Pretty,
    /// One JSON object per line.
    Json,
}

impl LogFormat {
    /// The format named by `ARENA_LOG_FORMAT`; unset means pretty.
    pub fn parse(value: Option<&str>) -> Result<Self, String> {
        match value.map(str::trim) {
            None | Some("" | "pretty") => Ok(Self::Pretty),
            Some("json") => Ok(Self::Json),
            Some(other) => Err(format!(
                "ARENA_LOG_FORMAT must be pretty or json, not {other:?}"
            )),
        }
    }
}

/// A match id written as the lobby and the join API show it.
#[derive(Clone, Copy)]
pub struct MatchId(pub u64);

impl fmt::Display for MatchId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:016x}", self.0)
    }
}

impl fmt::Debug for MatchId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

/// Installs the process-wide subscriber from `ARENA_LOG_FORMAT` and
/// `RUST_LOG`. A bad format falls back to pretty and says so, so a typo
/// never silences the server. Calling it again does nothing.
pub fn init() {
    let format = std::env::var("ARENA_LOG_FORMAT").ok();
    let (format, problem) = match LogFormat::parse(format.as_deref()) {
        Ok(format) => (format, None),
        Err(problem) => (LogFormat::Pretty, Some(problem)),
    };
    // Unset means `info`; a filter that does not parse also means `info`,
    // with a warning once the subscriber is up.
    let (filter, bad_filter) = match std::env::var("RUST_LOG") {
        Err(_) => (EnvFilter::new("info"), None),
        Ok(text) => match EnvFilter::try_new(&text) {
            Ok(filter) => (filter, None),
            Err(error) => (EnvFilter::new("info"), Some(error.to_string())),
        },
    };
    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(format == LogFormat::Pretty && std::io::stderr().is_terminal())
        .with_writer(std::io::stderr);
    let installed = match format {
        LogFormat::Pretty => builder.try_init(),
        LogFormat::Json => builder
            .json()
            .flatten_event(true)
            .with_current_span(false)
            .with_span_list(false)
            .try_init(),
    };
    if installed.is_err() {
        return;
    }
    if let Some(problem) = problem {
        tracing::warn!(reason = %problem, "unknown log format; using pretty");
    }
    if let Some(problem) = bad_filter {
        tracing::warn!(reason = %problem, "RUST_LOG does not parse; using info");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_format_defaults_to_pretty_and_refuses_unknown_names() {
        assert_eq!(LogFormat::parse(None), Ok(LogFormat::Pretty));
        assert_eq!(LogFormat::parse(Some("")), Ok(LogFormat::Pretty));
        assert_eq!(LogFormat::parse(Some("pretty")), Ok(LogFormat::Pretty));
        assert_eq!(LogFormat::parse(Some(" json ")), Ok(LogFormat::Json));
        let refused = LogFormat::parse(Some("yaml")).unwrap_err();
        assert!(refused.contains("ARENA_LOG_FORMAT"), "{refused}");
    }

    #[test]
    fn match_ids_print_as_sixteen_hex_digits() {
        assert_eq!(MatchId(0xabc).to_string(), "0000000000000abc");
        assert_eq!(format!("{:?}", MatchId(u64::MAX)), "ffffffffffffffff");
    }
}
