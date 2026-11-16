//! Process-wide counters, served as Prometheus text at `/metrics`.

use std::fmt::Write;
use std::sync::atomic::{AtomicU64, Ordering};

use arena_protocol::ErrorCode;

const CODES: [ErrorCode; 9] = [
    ErrorCode::VersionMismatch,
    ErrorCode::BadTicket,
    ErrorCode::MatchFull,
    ErrorCode::MatchOver,
    ErrorCode::RateLimited,
    ErrorCode::Malformed,
    ErrorCode::HelloTimeout,
    ErrorCode::ServerShutdown,
    ErrorCode::Replaced,
];

#[derive(Debug, Default)]
pub struct Metrics {
    pub matches_active: AtomicU64,
    pub matches_created: AtomicU64,
    pub connections_open: AtomicU64,
    pub ticks: AtomicU64,
    /// Ticks whose processing took longer than one tick interval.
    pub tick_overruns: AtomicU64,
    /// Slowest single tick, microseconds.
    pub tick_max_us: AtomicU64,
    pub inputs: AtomicU64,
    pub inputs_stale: AtomicU64,
    pub inputs_overflow: AtomicU64,
    /// Inputs folded into an earlier one to drain a backlog.
    pub inputs_folded: AtomicU64,
    /// Ticks where a connected seat had no input ready (it stood idle).
    pub inputs_starved: AtomicU64,
    /// Outgoing frames dropped because a client's send queue was full.
    pub frames_dropped: AtomicU64,
    pub bytes_out: AtomicU64,
    errors: [AtomicU64; CODES.len()],
    /// Ticks by how many inputs a connected seat received since the last
    /// one: 0, 1, 2, 3 and 4 or more. Steady clients are almost all 1;
    /// clumped delivery (jitter) shows as 0s and 2+s.
    arrivals: [AtomicU64; 5],
}

impl Metrics {
    pub fn add(counter: &AtomicU64, amount: u64) {
        counter.fetch_add(amount, Ordering::Relaxed);
    }
    pub fn sub(counter: &AtomicU64, amount: u64) {
        counter.fetch_sub(amount, Ordering::Relaxed);
    }
    pub fn get(counter: &AtomicU64) -> u64 {
        counter.load(Ordering::Relaxed)
    }
    pub fn record_error(&self, code: ErrorCode) {
        let index = CODES.iter().position(|known| *known == code).unwrap_or(0);
        Self::add(&self.errors[index], 1);
    }
    pub fn errors(&self, code: ErrorCode) -> u64 {
        CODES
            .iter()
            .position(|known| *known == code)
            .map_or(0, |index| Self::get(&self.errors[index]))
    }
    pub fn record_arrivals(&self, inputs: u32) {
        Self::add(&self.arrivals[(inputs as usize).min(4)], 1);
    }
    pub fn arrivals(&self) -> [u64; 5] {
        std::array::from_fn(|index| Self::get(&self.arrivals[index]))
    }
    pub fn record_tick(&self, micros: u64, interval_micros: u64) {
        self.tick_max_us.fetch_max(micros, Ordering::Relaxed);
        if micros > interval_micros {
            Self::add(&self.tick_overruns, 1);
        }
    }

    pub fn render(&self) -> String {
        let mut text = String::new();
        let mut line = |name: &str, kind: &str, help: &str, value: u64| {
            let _ = writeln!(text, "# HELP arena_{name} {help}");
            let _ = writeln!(text, "# TYPE arena_{name} {kind}");
            let _ = writeln!(text, "arena_{name} {value}");
        };
        let get = Self::get;
        line(
            "matches_active",
            "gauge",
            "Matches currently hosted.",
            get(&self.matches_active),
        );
        line(
            "matches_created_total",
            "counter",
            "Matches created.",
            get(&self.matches_created),
        );
        line(
            "connections_open",
            "gauge",
            "Open WebSocket connections.",
            get(&self.connections_open),
        );
        line(
            "ticks_total",
            "counter",
            "Rule ticks advanced across all matches.",
            get(&self.ticks),
        );
        line(
            "tick_overruns_total",
            "counter",
            "Ticks slower than the tick interval.",
            get(&self.tick_overruns),
        );
        line(
            "tick_max_microseconds",
            "gauge",
            "Slowest single tick since start.",
            get(&self.tick_max_us),
        );
        line(
            "inputs_total",
            "counter",
            "Inputs queued for a tick.",
            get(&self.inputs),
        );
        line(
            "inputs_stale_total",
            "counter",
            "Inputs dropped for a repeated or old seq.",
            get(&self.inputs_stale),
        );
        line(
            "inputs_overflow_total",
            "counter",
            "Inputs dropped by the per-seat queue bound.",
            get(&self.inputs_overflow),
        );
        line(
            "inputs_folded_total",
            "counter",
            "Inputs folded together to drain a backlog.",
            get(&self.inputs_folded),
        );
        line(
            "inputs_starved_total",
            "counter",
            "Ticks where a connected seat had no input ready.",
            get(&self.inputs_starved),
        );
        line(
            "frames_dropped_total",
            "counter",
            "Outgoing frames dropped for slow clients.",
            get(&self.frames_dropped),
        );
        line(
            "bytes_out_total",
            "counter",
            "Encoded message bytes queued to clients.",
            get(&self.bytes_out),
        );
        let _ = writeln!(
            text,
            "# HELP arena_input_arrivals_total Ticks by inputs a connected seat received since the last tick."
        );
        let _ = writeln!(text, "# TYPE arena_input_arrivals_total counter");
        for (count, counter) in ["0", "1", "2", "3", "4+"].iter().zip(&self.arrivals) {
            let _ = writeln!(
                text,
                "arena_input_arrivals_total{{per_tick=\"{count}\"}} {}",
                get(counter)
            );
        }
        let _ = writeln!(
            text,
            "# HELP arena_errors_total Error messages sent before closing, by code."
        );
        let _ = writeln!(text, "# TYPE arena_errors_total counter");
        for (code, counter) in CODES.iter().zip(&self.errors) {
            let _ = writeln!(
                text,
                "arena_errors_total{{code=\"{code:?}\"}} {}",
                get(counter)
            );
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_render_as_prometheus_text() {
        let metrics = Metrics::default();
        Metrics::add(&metrics.inputs, 3);
        metrics.record_error(ErrorCode::RateLimited);
        metrics.record_tick(40_000, 33_333);
        metrics.record_tick(10, 33_333);
        let text = metrics.render();
        assert!(text.contains("arena_inputs_total 3\n"));
        assert!(text.contains("arena_errors_total{code=\"RateLimited\"} 1\n"));
        assert!(text.contains("arena_tick_overruns_total 1\n"));
        assert!(text.contains("arena_tick_max_microseconds 40000\n"));
        assert_eq!(metrics.errors(ErrorCode::RateLimited), 1);
        assert_eq!(metrics.errors(ErrorCode::BadTicket), 0);
    }
}
