//! Process logs for `arena-world` and `arena-door` (ADR 0054).
//!
//! Every line is a `tracing` event on stderr. `ARENA_LOG_FORMAT` picks the
//! shape: `pretty` (the default) for a terminal, `json` (one object per line,
//! as the Docker deployment sets) for log shippers, matching Caddy's JSON
//! access log. `RUST_LOG` filters as usual and defaults to `info`.
//!
//! Lifecycle events carry the same field names everywhere, so one match or
//! connection can be followed from the front door to the world server:
//! `match_id` (16 hex digits, as the lobby shows it), `seat`, `conn_id`,
//! `team_size`, `mode`, `error_code` and `reason`.
//!
//! Never log a ticket, a join link or `ARENA_TICKET_KEY`: tickets are bearer
//! credentials. Nothing here logs per tick or per input; the hot path
//! reports through `/metrics` only.

use std::fmt;
use std::io::IsTerminal;

use opentelemetry::trace::{TraceContextExt, TracerProvider as _};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::SdkTracerProvider;
use tracing::{Level, Span};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer, Registry};

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

/// The profiling agent logs every upload at `info` through the `log` bridge;
/// keep it to warnings unless `RUST_LOG` names it.
fn with_quiet_agents(filter: &str) -> String {
    if filter.contains("pyroscope") {
        filter.to_owned()
    } else {
        format!("{filter},pyroscope=warn")
    }
}

/// The OTLP endpoint to export traces to: `OTEL_EXPORTER_OTLP_ENDPOINT` when
/// set and not blank. `None` means no exporter, thread or socket is created.
pub fn otlp_endpoint(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|endpoint| !endpoint.is_empty())
}

/// What [`init`] installed; flush it with [`Telemetry::shutdown`] before
/// the process exits so the last spans are exported.
#[must_use = "call shutdown() before exiting so buffered spans are exported"]
pub struct Telemetry {
    provider: Option<SdkTracerProvider>,
}

impl Telemetry {
    /// Whether spans are exported over OTLP.
    pub fn exporting(&self) -> bool {
        self.provider.is_some()
    }

    /// Exports buffered spans and stops the exporter. Call it outside any
    /// async runtime: the HTTP exporter blocks.
    pub fn shutdown(self) {
        if let Some(provider) = self.provider
            && let Err(error) = provider.shutdown()
        {
            eprintln!("telemetry: span export did not shut down cleanly: {error}");
        }
    }
}

/// Spans exported over OTLP: the services' own control-plane spans and the
/// HTTP request spans, whatever `RUST_LOG` says about log lines.
fn exported_spans() -> Targets {
    Targets::new()
        .with_target("arena_world", Level::INFO)
        .with_target("arena_door", Level::INFO)
        .with_target("tower_http", Level::INFO)
}

/// Builds the OTLP/HTTP span exporter for `service`. The endpoint, headers
/// and timeouts come from the standard `OTEL_EXPORTER_OTLP_*` variables.
fn tracer_provider(service: &'static str) -> Result<SdkTracerProvider, String> {
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .build()
        .map_err(|error| error.to_string())?;
    let mut resource = Resource::builder();
    if std::env::var_os("OTEL_SERVICE_NAME").is_none() {
        resource = resource.with_service_name(service);
    }
    Ok(SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(resource.build())
        .build())
}

/// Installs the process-wide subscriber from `ARENA_LOG_FORMAT`, `RUST_LOG`
/// and `OTEL_EXPORTER_OTLP_ENDPOINT`, naming this process `service` in
/// exported spans. A bad format or filter falls back to the default and says
/// so, so a typo never silences the server. Call it before starting an async
/// runtime (the OTLP HTTP client is blocking). Calling it again installs
/// nothing.
pub fn init(service: &'static str) -> Telemetry {
    let format = std::env::var("ARENA_LOG_FORMAT").ok();
    let (format, problem) = match LogFormat::parse(format.as_deref()) {
        Ok(format) => (format, None),
        Err(problem) => (LogFormat::Pretty, Some(problem)),
    };
    // Unset means `info`; a filter that does not parse also means `info`,
    // with a warning once the subscriber is up.
    let (filter, bad_filter) = match std::env::var("RUST_LOG") {
        Err(_) => (EnvFilter::new(with_quiet_agents("info")), None),
        Ok(text) => match EnvFilter::try_new(with_quiet_agents(&text)) {
            Ok(filter) => (filter, None),
            Err(error) => (
                EnvFilter::new(with_quiet_agents("info")),
                Some(error.to_string()),
            ),
        },
    };
    let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok();
    let (provider, export_problem) = match otlp_endpoint(endpoint.as_deref()) {
        None => (None, None),
        Some(_) => match tracer_provider(service) {
            Ok(provider) => (Some(provider), None),
            Err(problem) => (None, Some(problem)),
        },
    };
    let ansi = format == LogFormat::Pretty && std::io::stderr().is_terminal();
    let lines: Box<dyn Layer<Registry> + Send + Sync> = match format {
        LogFormat::Pretty => tracing_subscriber::fmt::layer()
            .with_ansi(ansi)
            .with_writer(std::io::stderr)
            .with_filter(filter)
            .boxed(),
        // Lines inside a control-plane span carry it under `span`, with the
        // `trace_id` and `span_id` Grafana links to Tempo.
        LogFormat::Json => tracing_subscriber::fmt::layer()
            .json()
            .flatten_event(true)
            .with_current_span(true)
            .with_span_list(false)
            .with_writer(std::io::stderr)
            .with_filter(filter)
            .boxed(),
    };
    let mut layers = vec![lines];
    if let Some(provider) = &provider {
        layers.push(
            tracing_opentelemetry::layer()
                .with_tracer(provider.tracer("arena"))
                .with_filter(exported_spans())
                .boxed(),
        );
    }
    if tracing_subscriber::registry()
        .with(layers)
        .try_init()
        .is_err()
    {
        return Telemetry { provider };
    }
    if let Some(problem) = problem {
        tracing::warn!(reason = %problem, "unknown log format; using pretty");
    }
    if let Some(problem) = bad_filter {
        tracing::warn!(reason = %problem, "RUST_LOG does not parse; using info");
    }
    if let Some(problem) = export_problem {
        tracing::warn!(reason = %problem, "OTLP span export is off: the exporter did not start");
    } else if provider.is_some() {
        tracing::info!(service, "exporting spans over OTLP");
    }
    Telemetry { provider }
}

/// Records the OpenTelemetry trace and span ids of `span` into its
/// `trace_id` and `span_id` fields, so log lines inside it name the trace.
/// Without an exporter the ids are invalid and nothing is recorded. Use it
/// through [`control_span!`](crate::control_span).
pub fn with_trace_ids(span: Span) -> Span {
    let context = span.context();
    let otel = context.span();
    let ids = otel.span_context();
    if ids.is_valid() {
        span.record("trace_id", tracing::field::display(ids.trace_id()));
        span.record("span_id", tracing::field::display(ids.span_id()));
    }
    span
}

/// An `info` span for one control-plane step (never per tick or per input)
/// whose log lines carry its trace ids: `control_span!("match.start",
/// match_id = %id)` or `control_span!(parent: &search, "ready_check.accept")`.
#[macro_export]
macro_rules! control_span {
    (parent: $parent:expr, $name:expr $(, $($fields:tt)*)?) => {
        $crate::logging::with_trace_ids(::tracing::info_span!(
            parent: $parent,
            $name,
            trace_id = ::tracing::field::Empty,
            span_id = ::tracing::field::Empty
            $(, $($fields)*)?
        ))
    };
    ($name:expr $(, $($fields:tt)*)?) => {
        $crate::logging::with_trace_ids(::tracing::info_span!(
            $name,
            trace_id = ::tracing::field::Empty,
            span_id = ::tracing::field::Empty
            $(, $($fields)*)?
        ))
    };
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

    #[test]
    fn the_profiling_agent_is_quiet_unless_asked_for() {
        assert_eq!(with_quiet_agents("info"), "info,pyroscope=warn");
        assert_eq!(
            with_quiet_agents("debug,pyroscope=info"),
            "debug,pyroscope=info"
        );
        assert!(EnvFilter::try_new(with_quiet_agents("arena_world=debug")).is_ok());
    }

    #[test]
    fn a_blank_endpoint_means_no_exporter() {
        assert_eq!(otlp_endpoint(None), None);
        assert_eq!(otlp_endpoint(Some("")), None);
        assert_eq!(otlp_endpoint(Some("  ")), None);
        assert_eq!(
            otlp_endpoint(Some(" http://alloy:4318 ")),
            Some("http://alloy:4318")
        );
    }

    #[derive(Clone, Default)]
    struct Captured(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
    impl std::io::Write for Captured {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl Captured {
        fn lines(&self) -> Vec<serde_json::Value> {
            String::from_utf8(self.0.lock().unwrap().clone())
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect()
        }
    }

    fn json_lines(captured: &Captured) -> impl Layer<Registry> + Send + Sync {
        let writer = captured.clone();
        tracing_subscriber::fmt::layer()
            .json()
            .flatten_event(true)
            .with_current_span(true)
            .with_span_list(false)
            .with_writer(move || writer.clone())
    }

    #[test]
    fn log_lines_in_a_control_span_name_the_exported_trace() {
        use opentelemetry_sdk::trace::InMemorySpanExporter;
        let exporter = InMemorySpanExporter::default();
        let provider = SdkTracerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .build();
        let captured = Captured::default();
        let subscriber = tracing_subscriber::registry()
            .with(json_lines(&captured))
            .with(
                tracing_opentelemetry::layer()
                    .with_tracer(provider.tracer("test"))
                    .with_filter(exported_spans()),
            );
        tracing::subscriber::with_default(subscriber, || {
            let outer = crate::control_span!("match.start", match_id = %MatchId(7));
            let inner = crate::control_span!(parent: &outer, "seat.join", seat = 1u64);
            inner.in_scope(|| tracing::info!("seat joined"));
        });
        provider.force_flush().unwrap();
        let spans = exporter.get_finished_spans().unwrap();
        let exported = |name: &str| {
            spans
                .iter()
                .find(|span| span.name == name)
                .unwrap_or_else(|| panic!("no {name} span in {spans:?}"))
        };
        let (outer, inner) = (exported("match.start"), exported("seat.join"));
        assert_eq!(inner.span_context.trace_id(), outer.span_context.trace_id());
        assert_eq!(inner.parent_span_id, outer.span_context.span_id());
        let lines = captured.lines();
        let line = lines
            .iter()
            .find(|line| line["message"] == "seat joined")
            .unwrap();
        assert_eq!(line["span"]["name"], "seat.join");
        assert_eq!(
            line["span"]["trace_id"],
            inner.span_context.trace_id().to_string()
        );
        assert_eq!(
            line["span"]["span_id"],
            inner.span_context.span_id().to_string()
        );
    }

    #[test]
    fn without_an_exporter_control_spans_record_no_ids() {
        let captured = Captured::default();
        let subscriber = tracing_subscriber::registry().with(json_lines(&captured));
        tracing::subscriber::with_default(subscriber, || {
            crate::control_span!("match.start").in_scope(|| tracing::info!("started"));
        });
        let lines = captured.lines();
        assert_eq!(lines[0]["span"]["name"], "match.start");
        assert!(lines[0]["span"].get("trace_id").is_none(), "{}", lines[0]);
    }
}
