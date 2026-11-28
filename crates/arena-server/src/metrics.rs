//! Process-wide counters, served as Prometheus text at `/metrics`.

use std::fmt::{Display, Write};
use std::sync::atomic::{AtomicU64, Ordering};

use arena_protocol::ErrorCode;
use arena_sim::arena::RoundResult;

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

/// Upper bounds of `arena_tick_duration_microseconds`. Ticks usually take
/// single-digit microseconds, so the low buckets give real quantiles; the
/// budget at 30 Hz is 33 333 µs.
pub const TICK_BUCKETS_US: [u64; 17] = [
    1, 2, 5, 10, 20, 50, 100, 250, 500, 1_000, 2_500, 5_000, 10_000, 16_667, 33_333, 66_667,
    100_000,
];

/// Appends one unlabelled series in the Prometheus text format.
pub fn write_metric(text: &mut String, name: &str, kind: &str, help: &str, value: impl Display) {
    let _ = writeln!(text, "# HELP {name} {help}");
    let _ = writeln!(text, "# TYPE {name} {kind}");
    let _ = writeln!(text, "{name} {value}");
}

/// A fixed-bucket Prometheus histogram that records with two relaxed atomic
/// adds and a short scan, without locks or allocation. `N` bucket upper
/// bounds (inclusive, ascending) plus an implicit `+Inf`. Aligned to its own
/// cache lines so concurrent matches recording into it do not falsely share
/// with neighbouring counters.
#[derive(Debug)]
#[repr(align(64))]
pub struct Histogram<const N: usize> {
    /// Read-only, so kept out of the counters' cache lines.
    bounds: &'static [u64; N],
    /// Per-bucket (not cumulative) counts; index `N` is `+Inf`.
    counts: [AtomicU64; N],
    over: AtomicU64,
    sum: AtomicU64,
}

impl<const N: usize> Histogram<N> {
    pub fn new(bounds: &'static [u64; N]) -> Self {
        debug_assert!(bounds.windows(2).all(|pair| pair[0] < pair[1]));
        Self {
            bounds,
            counts: std::array::from_fn(|_| AtomicU64::new(0)),
            over: AtomicU64::new(0),
            sum: AtomicU64::new(0),
        }
    }

    pub fn record(&self, value: u64) {
        let bucket = self.bounds.partition_point(|bound| *bound < value);
        let counter = self.counts.get(bucket).unwrap_or(&self.over);
        counter.fetch_add(1, Ordering::Relaxed);
        self.sum.fetch_add(value, Ordering::Relaxed);
    }

    /// Cumulative counts per bound and then `+Inf`, as Prometheus reads them.
    pub fn cumulative(&self) -> ([u64; N], u64) {
        let mut total = 0u64;
        let buckets = std::array::from_fn(|index| {
            total += self.counts[index].load(Ordering::Relaxed);
            total
        });
        (buckets, total + self.over.load(Ordering::Relaxed))
    }

    pub fn sum(&self) -> u64 {
        self.sum.load(Ordering::Relaxed)
    }

    /// Appends `arena_<name>` in the Prometheus text format. `_count` is the
    /// `+Inf` bucket read in the same pass, so the two always agree.
    pub fn render(&self, text: &mut String, name: &str, help: &str) {
        let _ = writeln!(text, "# HELP arena_{name} {help}");
        let _ = writeln!(text, "# TYPE arena_{name} histogram");
        let (buckets, count) = self.cumulative();
        for (bound, cumulative) in self.bounds.iter().zip(buckets) {
            let _ = writeln!(text, "arena_{name}_bucket{{le=\"{bound}\"}} {cumulative}");
        }
        let _ = writeln!(text, "arena_{name}_bucket{{le=\"+Inf\"}} {count}");
        let _ = writeln!(text, "arena_{name}_sum {}", self.sum());
        let _ = writeln!(text, "arena_{name}_count {count}");
    }
}

#[derive(Debug, Default)]
pub struct Metrics {
    pub matches_active: AtomicU64,
    pub matches_created: AtomicU64,
    /// Hub worlds currently hosted (ADR 0057) and the players in them.
    pub worlds_active: AtomicU64,
    pub world_players: AtomicU64,
    /// Matches that reached a rules result: won, draw, time_limit_draw.
    matches_finished: [AtomicU64; 3],
    pub connections_open: AtomicU64,
    pub spectators_open: AtomicU64,
    /// Separate hot-path counters onto distinct cache lines to avoid cross-core
    /// false sharing under concurrent matches.
    _pad_ticks: [u64; 8],
    pub ticks: AtomicU64,
    /// Cumulative compute time of Running rule ticks, microseconds (1-to-1 with `ticks`).
    pub tick_total_us: AtomicU64,
    /// Ticks whose processing took longer than one tick interval.
    pub tick_overruns: AtomicU64,
    /// Slowest single tick, microseconds.
    _pad_max_us: [u64; 8],
    pub tick_max_us: AtomicU64,
    /// Running rule ticks by compute time; `_count`/`_sum` match
    /// `arena_ticks_total`/`arena_tick_microseconds_total`.
    pub tick_duration: TickHistogram,
    _pad_inputs: [u64; 8],
    pub inputs: AtomicU64,
    pub inputs_stale: AtomicU64,
    pub inputs_overflow: AtomicU64,
    /// Inputs folded into an earlier one to drain a backlog.
    pub inputs_folded: AtomicU64,
    /// Ticks where a connected seat had no input ready (it stood idle).
    pub inputs_starved: AtomicU64,
    /// Outgoing frames dropped because a client's send queue was full.
    pub frames_dropped: AtomicU64,
    _pad_bytes: [u64; 8],
    pub bytes_out: AtomicU64,
    /// Connections refused before upgrading for their page origin.
    pub refused_origin: AtomicU64,
    /// Connections refused for the per-address connection limit.
    pub refused_address: AtomicU64,
    errors: [AtomicU64; CODES.len()],
    /// Ticks by how many inputs a connected seat received since the last
    /// one: 0, 1, 2, 3 and 4 or more. Steady clients are almost all 1;
    /// clumped delivery (jitter) shows as 0s and 2+s.
    _pad_arrivals: [u64; 8],
    arrivals: [AtomicU64; 5],
}

/// `arena_tick_duration_microseconds`.
#[derive(Debug)]
pub struct TickHistogram(pub Histogram<{ TICK_BUCKETS_US.len() }>);

impl Default for TickHistogram {
    fn default() -> Self {
        Self(Histogram::new(&TICK_BUCKETS_US))
    }
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
    pub fn record_result(&self, result: RoundResult) {
        let index = match result {
            RoundResult::Won { .. } => 0,
            RoundResult::Draw => 1,
            RoundResult::TimeLimitDraw => 2,
        };
        Self::add(&self.matches_finished[index], 1);
    }
    pub fn finished(&self, result: RoundResult) -> u64 {
        let index = match result {
            RoundResult::Won { .. } => 0,
            RoundResult::Draw => 1,
            RoundResult::TimeLimitDraw => 2,
        };
        Self::get(&self.matches_finished[index])
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
    pub fn record_arrivals_batch(&self, counts: [u64; 5]) {
        for (counter, count) in self.arrivals.iter().zip(counts) {
            if count > 0 {
                Self::add(counter, count);
            }
        }
    }
    pub fn arrivals(&self) -> [u64; 5] {
        std::array::from_fn(|index| Self::get(&self.arrivals[index]))
    }
    /// Records tick overrun and max duration for any session interval tick
    /// (including waiting/linger ticks that do not advance `arena_ticks_total`).
    pub fn record_tick(&self, micros: u64, interval_micros: u64) {
        if micros > self.tick_max_us.load(Ordering::Relaxed) {
            self.tick_max_us.fetch_max(micros, Ordering::Relaxed);
        }
        if micros > interval_micros {
            Self::add(&self.tick_overruns, 1);
        }
    }
    /// Records a `Phase::Running` rule tick's compute time into
    /// `arena_tick_microseconds_total` (matching `arena_ticks_total` 1-to-1)
    /// as well as overrun and max gauges.
    pub fn record_running_tick(&self, micros: u64, interval_micros: u64) {
        Self::add(&self.tick_total_us, micros);
        self.tick_duration.0.record(micros);
        self.record_tick(micros, interval_micros);
    }

    pub fn render(&self) -> String {
        let mut text = String::new();
        let mut line = |name: &str, kind: &str, help: &str, value: u64| {
            write_metric(&mut text, &format!("arena_{name}"), kind, help, value);
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
            "worlds_active",
            "gauge",
            "Hub worlds currently hosted.",
            get(&self.worlds_active),
        );
        line(
            "world_players",
            "gauge",
            "Players in hub worlds, connected or within their grace.",
            get(&self.world_players),
        );
        line(
            "connections_open",
            "gauge",
            "Open WebSocket connections.",
            get(&self.connections_open),
        );
        line(
            "spectators_open",
            "gauge",
            "Open watch-only spectator connections.",
            get(&self.spectators_open),
        );
        line(
            "connections_refused_origin_total",
            "counter",
            "Connections refused because their page origin is not allowed.",
            get(&self.refused_origin),
        );
        line(
            "connections_refused_address_total",
            "counter",
            "Connections refused because their address holds too many.",
            get(&self.refused_address),
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
            "tick_microseconds_total",
            "counter",
            "Cumulative compute time of running rule ticks in microseconds.",
            get(&self.tick_total_us),
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
        self.tick_duration.0.render(
            &mut text,
            "tick_duration_microseconds",
            "Compute time of running rule ticks in microseconds.",
        );
        let _ = writeln!(
            text,
            "# HELP arena_matches_finished_total Matches that reached a rules result, by outcome."
        );
        let _ = writeln!(text, "# TYPE arena_matches_finished_total counter");
        for (label, counter) in ["won", "draw", "time_limit_draw"]
            .iter()
            .zip(&self.matches_finished)
        {
            let _ = writeln!(
                text,
                "arena_matches_finished_total{{result=\"{label}\"}} {}",
                get(counter)
            );
        }
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

/// Linux `USER_HZ`: the clock-tick unit of `/proc/<pid>/stat` times, fixed
/// at 100 for user space on every architecture Linux ships.
const USER_HZ: f64 = 100.0;

/// Resident memory, CPU time and open descriptors of this process, read from
/// `/proc/self` at scrape time (never on the tick path). Empty off Linux or
/// when `/proc` is unavailable.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ProcessStats {
    pub resident_bytes: Option<u64>,
    pub cpu_seconds: Option<f64>,
    pub open_fds: Option<u64>,
}

impl ProcessStats {
    pub fn read() -> Self {
        let status = std::fs::read_to_string("/proc/self/status").ok();
        let stat = std::fs::read_to_string("/proc/self/stat").ok();
        let open_fds = std::fs::read_dir("/proc/self/fd")
            .ok()
            .map(|entries| entries.count() as u64);
        Self {
            resident_bytes: status.as_deref().and_then(parse_resident_bytes),
            cpu_seconds: stat.as_deref().and_then(parse_cpu_seconds),
            open_fds,
        }
    }

    /// Appends the standard `process_*` series for whatever is known.
    pub fn render(&self, text: &mut String) {
        if let Some(bytes) = self.resident_bytes {
            write_metric(
                text,
                "process_resident_memory_bytes",
                "gauge",
                "Resident memory size in bytes.",
                bytes,
            );
        }
        if let Some(seconds) = self.cpu_seconds {
            write_metric(
                text,
                "process_cpu_seconds_total",
                "counter",
                "User and system CPU time in seconds.",
                seconds,
            );
        }
        if let Some(fds) = self.open_fds {
            write_metric(
                text,
                "process_open_fds",
                "gauge",
                "Open file descriptors (including the one used to count them).",
                fds,
            );
        }
    }
}

/// Appends the Tokio runtime's stable gauges (workers, alive tasks, global
/// queue depth) when called inside a runtime. Per-task poll metrics need
/// `tokio_unstable` and are not reported.
pub fn render_runtime(text: &mut String) {
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        return;
    };
    let runtime = handle.metrics();
    for (name, help, value) in [
        (
            "tokio_workers",
            "Worker threads of the Tokio runtime.",
            runtime.num_workers(),
        ),
        (
            "tokio_alive_tasks",
            "Tasks alive in the Tokio runtime.",
            runtime.num_alive_tasks(),
        ),
        (
            "tokio_global_queue_depth",
            "Tasks waiting in the Tokio runtime's global queue.",
            runtime.global_queue_depth(),
        ),
    ] {
        write_metric(text, name, "gauge", help, value);
    }
}

/// `VmRSS:    1234 kB` from `/proc/self/status`.
fn parse_resident_bytes(status: &str) -> Option<u64> {
    let line = status.lines().find(|line| line.starts_with("VmRSS:"))?;
    let kib: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    kib.checked_mul(1024)
}

/// `utime` + `stime` (fields 14 and 15) from `/proc/self/stat`. The command
/// name in field 2 may hold spaces and parentheses, so fields are counted
/// after its last `)`.
fn parse_cpu_seconds(stat: &str) -> Option<f64> {
    let rest = &stat[stat.rfind(')')? + 1..];
    let mut fields = rest.split_whitespace().skip(11);
    let user: u64 = fields.next()?.parse().ok()?;
    let system: u64 = fields.next()?.parse().ok()?;
    Some((user + system) as f64 / USER_HZ)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_render_as_prometheus_text() {
        let metrics = Metrics::default();
        Metrics::add(&metrics.inputs, 3);
        metrics.record_error(ErrorCode::RateLimited);
        metrics.record_result(RoundResult::Won { team: 0 });
        metrics.record_result(RoundResult::TimeLimitDraw);
        // Non-running ticks update max/overruns without inflating tick_microseconds_total.
        metrics.record_tick(40_000, 33_333);
        metrics.record_running_tick(150, 33_333);
        metrics.record_running_tick(50, 33_333);
        let text = metrics.render();
        assert!(text.contains("arena_inputs_total 3\n"));
        assert!(text.contains("arena_matches_finished_total{result=\"won\"} 1\n"));
        assert!(text.contains("arena_matches_finished_total{result=\"draw\"} 0\n"));
        assert!(text.contains("arena_matches_finished_total{result=\"time_limit_draw\"} 1\n"));
        assert!(text.contains("arena_errors_total{code=\"RateLimited\"} 1\n"));
        assert!(text.contains("arena_tick_overruns_total 1\n"));
        assert!(text.contains("arena_tick_max_microseconds 40000\n"));
        assert!(text.contains("arena_tick_microseconds_total 200\n"));
        // Only the two running ticks land in the histogram: 50 µs and 150 µs.
        assert!(text.contains("# TYPE arena_tick_duration_microseconds histogram\n"));
        assert!(text.contains("arena_tick_duration_microseconds_bucket{le=\"1\"} 0\n"));
        assert!(text.contains("arena_tick_duration_microseconds_bucket{le=\"20\"} 0\n"));
        assert!(text.contains("arena_tick_duration_microseconds_bucket{le=\"50\"} 1\n"));
        assert!(text.contains("arena_tick_duration_microseconds_bucket{le=\"100\"} 1\n"));
        assert!(text.contains("arena_tick_duration_microseconds_bucket{le=\"250\"} 2\n"));
        assert!(text.contains("arena_tick_duration_microseconds_bucket{le=\"+Inf\"} 2\n"));
        assert!(text.contains("arena_tick_duration_microseconds_sum 200\n"));
        assert!(text.contains("arena_tick_duration_microseconds_count 2\n"));
        assert_eq!(metrics.errors(ErrorCode::RateLimited), 1);
        assert_eq!(metrics.errors(ErrorCode::BadTicket), 0);
        assert_eq!(metrics.finished(RoundResult::TimeLimitDraw), 1);
        assert_eq!(metrics.finished(RoundResult::Draw), 0);
    }

    #[test]
    fn histogram_buckets_are_inclusive_cumulative_and_overflow_to_infinity() {
        static BOUNDS: [u64; 3] = [10, 20, 30];
        let histogram = Histogram::new(&BOUNDS);
        for value in [0, 10, 11, 20, 30, 31, 1_000_000] {
            histogram.record(value);
        }
        // <=10: 0, 10; <=20: 11, 20; <=30: 30; +Inf: 31 and the huge value.
        assert_eq!(histogram.cumulative(), ([2, 4, 5], 7));
        assert_eq!(histogram.sum(), 1_000_102);
        let mut text = String::new();
        histogram.render(&mut text, "demo", "A demo.");
        assert_eq!(
            text,
            "# HELP arena_demo A demo.\n# TYPE arena_demo histogram\n\
             arena_demo_bucket{le=\"10\"} 2\narena_demo_bucket{le=\"20\"} 4\n\
             arena_demo_bucket{le=\"30\"} 5\narena_demo_bucket{le=\"+Inf\"} 7\n\
             arena_demo_sum 1000102\narena_demo_count 7\n"
        );
    }

    #[test]
    fn concurrent_matches_record_every_tick_on_their_own_cache_lines() {
        assert_eq!(std::mem::align_of::<TickHistogram>(), 64);
        let metrics = Metrics::default();
        std::thread::scope(|scope| {
            for _ in 0..4 {
                scope.spawn(|| {
                    for micros in 0..1_000 {
                        metrics.record_running_tick(micros, 33_333);
                    }
                });
            }
        });
        let (buckets, count) = metrics.tick_duration.0.cumulative();
        assert_eq!(count, 4_000);
        // The first bucket (<= 1 µs) holds 0 and 1 from each thread.
        assert_eq!(buckets[0], 4 * 2);
        assert_eq!(buckets[TICK_BUCKETS_US.len() - 1], 4_000);
        assert_eq!(metrics.tick_duration.0.sum(), 4 * (0..1_000).sum::<u64>());
        assert_eq!(
            metrics.tick_duration.0.sum(),
            Metrics::get(&metrics.tick_total_us)
        );
    }

    #[test]
    fn process_stats_parse_proc_files() {
        let status = "Name:\tarena-server\nVmPeak:\t  9000 kB\nVmRSS:\t   2048 kB\nThreads:\t8\n";
        assert_eq!(parse_resident_bytes(status), Some(2048 * 1024));
        assert_eq!(parse_resident_bytes("Name:\tx\n"), None);
        // A command name with spaces and a parenthesis must not shift fields.
        let stat = "4242 (arena ) server) S 1 4242 4242 0 -1 4194560 100 0 0 0 250 50 0 0 20 0 8 0";
        assert_eq!(parse_cpu_seconds(stat), Some(3.0));
        assert_eq!(parse_cpu_seconds("garbage"), None);
        let mut text = String::new();
        ProcessStats {
            resident_bytes: Some(4096),
            cpu_seconds: Some(1.5),
            open_fds: None,
        }
        .render(&mut text);
        assert!(
            text.contains("process_resident_memory_bytes 4096\n"),
            "{text}"
        );
        assert!(text.contains("process_cpu_seconds_total 1.5\n"), "{text}");
        assert!(!text.contains("process_open_fds"), "{text}");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn this_process_reports_its_own_memory_cpu_and_descriptors() {
        let stats = ProcessStats::read();
        assert!(
            stats.resident_bytes.is_some_and(|bytes| bytes > 0),
            "{stats:?}"
        );
        assert!(stats.cpu_seconds.is_some(), "{stats:?}");
        assert!(stats.open_fds.is_some_and(|fds| fds > 0), "{stats:?}");
    }
}
