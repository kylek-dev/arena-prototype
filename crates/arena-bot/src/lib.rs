//! Bot players and load tester for `arena-server` (ADR 0044, Phase 4).
//!
//! A scenario runs `matches` concurrent match groups against a server in
//! development mode. Each group creates a match through `POST /dev/match`,
//! fills every seat with a bot, and starts a new match whenever a round
//! ends, until the run's duration is over. Bots only send ordinary inputs
//! and pings through a simulated [`link::Link`] and measure round trips,
//! input-to-acknowledgement latency, snapshot spacing and bandwidth. They
//! also cross-check that every seat of a match sees the same state hash for
//! the same tick.
//!
//! With [`Scenario::queue`], bots instead queue through the front door's
//! solo queues ([`queue`]) and play whatever match they are put in.

pub mod bot;
pub mod link;
pub mod queue;
pub mod stats;

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::Instant;

use bot::{Seat, SeatStats};
use link::{Link, Rng};
use stats::{Samples, Summary};

#[derive(Clone, Debug)]
pub struct Scenario {
    /// `host:port` of a server started with `--dev`.
    pub server: String,
    pub matches: usize,
    pub team_size: usize,
    pub duration: Duration,
    pub link: Link,
    /// Use browser-parity `Pacer` + `TickRate` adaptive input pacing.
    pub paced: bool,
    pub reconnect_every: Option<Duration>,
    /// How long a dropped seat stays away before rejoining.
    pub away: Duration,
    pub seed: u64,
    /// Linux only: measure this process's CPU time over the run.
    pub server_pid: Option<u32>,
    /// Queue through a front door instead of creating matches.
    pub queue: Option<queue::QueueScenario>,
}

/// State hashes seen per match and tick, to catch any divergence between
/// what different players are told.
#[derive(Debug, Default)]
pub struct Shared {
    hashes: Mutex<HashMap<u64, BTreeMap<u64, u64>>>,
    checks: AtomicU64,
    mismatches: AtomicU64,
}
impl Shared {
    pub fn check_hash(&self, match_id: u64, tick: u64, hash: u64) {
        let mut all = self.hashes.lock().expect("hash table lock");
        let ticks = all.entry(match_id).or_default();
        match ticks.get(&tick) {
            Some(seen) => {
                self.checks.fetch_add(1, Ordering::Relaxed);
                if *seen != hash {
                    self.mismatches.fetch_add(1, Ordering::Relaxed);
                }
            }
            None => {
                ticks.insert(tick, hash);
                // About 20 s of history covers any lagging seat.
                while ticks.len() > 600 {
                    ticks.pop_first();
                }
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct NewMatch {
    match_id: u64,
    tickets: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct ServerMetrics {
    pub ticks_total: u64,
    pub tick_overruns_total: u64,
    pub tick_max_microseconds: u64,
    pub frames_dropped_total: u64,
    pub inputs_overflow_total: u64,
    pub inputs_stale_total: u64,
    pub inputs_folded_total: u64,
    pub inputs_starved_total: u64,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub server: String,
    /// The queue the bots joined, when they queued through a front door.
    pub queue: Option<String>,
    pub matches: usize,
    pub team_size: usize,
    pub players: usize,
    pub seconds: f64,
    pub rtt_ms: f64,
    pub jitter_ms: f64,
    pub loss_pct: f64,
    pub clump_ms: f64,
    pub paced: bool,
    pub reconnect_every_s: Option<f64>,
    pub rounds_started: u64,
    pub rounds_finished: u64,
    pub rtt: Summary,
    pub input_latency: Summary,
    pub snapshot_gap: Summary,
    pub snapshots: u64,
    pub inputs: u64,
    pub kbit_per_s_in_per_player: f64,
    pub hash_checks: u64,
    pub hash_mismatches: u64,
    pub errors: BTreeMap<String, u64>,
    pub unexpected_closes: u64,
    pub lost: BTreeMap<String, u64>,
    pub rejoins_attempted: u64,
    pub rejoins_succeeded: u64,
    pub rejoins_too_late: u64,
    pub snapshots_showing_disconnected: u64,
    /// Snapshots whose result disagreed with the fighters' health.
    pub wrong_results: u64,
    /// Inputs the bots predicted, as a browser client does.
    pub predicted_inputs: u64,
    /// Reconciliations that moved a prediction.
    pub corrections: u64,
    /// Correction sizes in millimetres: [p50, p99, max].
    pub correction_mm: [u32; 3],
    pub server_metrics: Option<ServerMetrics>,
    /// Server CPU time divided by wall time: cores in use on average.
    pub server_cpu_cores: Option<f64>,
}

#[derive(Default)]
pub(crate) struct Totals {
    rtt: Samples,
    input_latency: Samples,
    snapshot_gap: Samples,
    snapshots: u64,
    inputs: u64,
    bytes_in: u64,
    errors: BTreeMap<String, u64>,
    unexpected_closes: u64,
    lost: BTreeMap<String, u64>,
    rejoins_attempted: u64,
    rejoins_succeeded: u64,
    rejoins_too_late: u64,
    saw_disconnected: u64,
    wrong_results: u64,
    predicted_inputs: u64,
    corrections: u64,
    correction_mm: Vec<u32>,
    rounds_started: u64,
    rounds_finished: u64,
}
impl Totals {
    fn add(&mut self, seat: SeatStats) {
        self.rtt.extend(seat.rtt);
        self.input_latency.extend(seat.input_latency);
        self.snapshot_gap.extend(seat.snapshot_gap);
        self.snapshots += seat.snapshots;
        self.inputs += seat.inputs;
        self.bytes_in += seat.bytes_in;
        for (code, count) in seat.errors {
            *self.errors.entry(code).or_default() += count;
        }
        self.unexpected_closes += seat.unexpected_closes;
        for (reason, count) in seat.lost {
            *self.lost.entry(reason.to_owned()).or_default() += count;
        }
        self.rejoins_attempted += seat.rejoins_attempted;
        self.rejoins_succeeded += seat.rejoins_succeeded;
        self.rejoins_too_late += seat.rejoins_too_late;
        self.saw_disconnected += seat.saw_disconnected;
        self.wrong_results += seat.wrong_results;
        self.predicted_inputs += seat.predicted_inputs;
        self.corrections += seat.corrections;
        self.correction_mm.extend(seat.correction_mm);
    }
    fn merge(&mut self, other: Totals) {
        self.rtt.extend(other.rtt);
        self.input_latency.extend(other.input_latency);
        self.snapshot_gap.extend(other.snapshot_gap);
        self.snapshots += other.snapshots;
        self.inputs += other.inputs;
        self.bytes_in += other.bytes_in;
        for (code, count) in other.errors {
            *self.errors.entry(code).or_default() += count;
        }
        self.unexpected_closes += other.unexpected_closes;
        for (reason, count) in other.lost {
            *self.lost.entry(reason).or_default() += count;
        }
        self.rejoins_attempted += other.rejoins_attempted;
        self.rejoins_succeeded += other.rejoins_succeeded;
        self.rejoins_too_late += other.rejoins_too_late;
        self.saw_disconnected += other.saw_disconnected;
        self.wrong_results += other.wrong_results;
        self.predicted_inputs += other.predicted_inputs;
        self.corrections += other.corrections;
        self.correction_mm.extend(other.correction_mm);
        self.rounds_started += other.rounds_started;
        self.rounds_finished += other.rounds_finished;
    }
}

pub async fn run(mut scenario: Scenario) -> Result<Report, String> {
    if let Some(queue) = &scenario.queue {
        scenario.team_size = match queue.mode.as_str() {
            "1v1" => 1,
            "2v2" => 2,
            "3v3" => 3,
            other => return Err(format!("unknown queue {other}: use 1v1, 2v2 or 3v3")),
        };
        if queue.bots == 0 {
            return Err("need at least one bot".into());
        }
    }
    if !(1..=3).contains(&scenario.team_size) || scenario.matches == 0 {
        return Err("need at least one match and a team size of 1, 2 or 3".into());
    }
    let shared = std::sync::Arc::new(Shared::default());
    let layout = std::sync::Arc::new(arena_map::colosseum().layout);
    let started = Instant::now();
    let deadline = started + scenario.duration;
    let cpu_start = scenario.server_pid.and_then(cpu_seconds);
    let groups: Vec<_> = match scenario.queue.clone() {
        Some(queue) => (0..queue.bots)
            .map(|bot| {
                tokio::spawn(queue::run_bot(
                    scenario.clone(),
                    queue.clone(),
                    bot as u64,
                    deadline,
                    shared.clone(),
                    layout.clone(),
                ))
            })
            .collect(),
        None => (0..scenario.matches)
            .map(|group| {
                tokio::spawn(run_group(
                    scenario.clone(),
                    group as u64,
                    deadline,
                    shared.clone(),
                    layout.clone(),
                ))
            })
            .collect(),
    };
    let mut totals = Totals::default();
    for group in groups {
        totals.merge(group.await.map_err(|error| error.to_string())??);
    }
    let seconds = started.elapsed().as_secs_f64();
    let server_cpu_cores = scenario
        .server_pid
        .and_then(cpu_seconds)
        .zip(cpu_start)
        .map(|(end, start)| (end - start) / seconds);
    let server_metrics = http(&scenario.server, "GET", "/metrics")
        .await
        .ok()
        .filter(|(status, _)| *status == 200)
        .map(|(_, body)| parse_metrics(&body));
    let players = match &scenario.queue {
        Some(queue) => queue.bots,
        None => scenario.matches * scenario.team_size * 2,
    };
    Ok(Report {
        server: scenario.server,
        queue: scenario
            .queue
            .as_ref()
            .map(|queue| format!("{} via {}", queue.mode, queue.door)),
        matches: if scenario.queue.is_some() {
            0
        } else {
            scenario.matches
        },
        team_size: scenario.team_size,
        players,
        seconds,
        rtt_ms: (scenario.link.one_way * 2).as_secs_f64() * 1000.0,
        jitter_ms: scenario.link.jitter.as_secs_f64() * 1000.0,
        loss_pct: scenario.link.loss_rate * 100.0,
        clump_ms: scenario.link.clump.as_secs_f64() * 1000.0,
        paced: scenario.paced,
        reconnect_every_s: scenario.reconnect_every.map(|every| every.as_secs_f64()),
        rounds_started: totals.rounds_started,
        rounds_finished: totals.rounds_finished,
        rtt: totals.rtt.summary(),
        input_latency: totals.input_latency.summary(),
        snapshot_gap: totals.snapshot_gap.summary(),
        snapshots: totals.snapshots,
        inputs: totals.inputs,
        kbit_per_s_in_per_player: totals.bytes_in as f64 * 8.0 / 1000.0 / players as f64 / seconds,
        hash_checks: shared.checks.load(Ordering::Relaxed),
        hash_mismatches: shared.mismatches.load(Ordering::Relaxed),
        errors: totals.errors,
        unexpected_closes: totals.unexpected_closes,
        lost: totals.lost,
        rejoins_attempted: totals.rejoins_attempted,
        rejoins_succeeded: totals.rejoins_succeeded,
        rejoins_too_late: totals.rejoins_too_late,
        snapshots_showing_disconnected: totals.saw_disconnected,
        wrong_results: totals.wrong_results,
        predicted_inputs: totals.predicted_inputs,
        corrections: totals.corrections,
        correction_mm: percentiles(totals.correction_mm),
        server_metrics,
        server_cpu_cores,
    })
}

/// One match slot: plays rounds back to back until the deadline.
async fn run_group(
    scenario: Scenario,
    group: u64,
    deadline: Instant,
    shared: std::sync::Arc<Shared>,
    layout: std::sync::Arc<arena_sim::map::Layout>,
) -> Result<Totals, String> {
    let mut totals = Totals::default();
    let mut rng = Rng::new(scenario.seed ^ group.wrapping_mul(0xA24B_AED4_963E_E407));
    while Instant::now() < deadline {
        let path = format!("/dev/match?team_size={}", scenario.team_size);
        let (status, body) = http(&scenario.server, "POST", &path).await?;
        if status != 200 {
            return Err(format!("POST {path} answered {status}: {body}"));
        }
        let created: NewMatch = serde_json::from_str(&body)
            .map_err(|error| format!("bad /dev/match reply: {error}"))?;
        totals.rounds_started += 1;
        let seats: Vec<_> = created
            .tickets
            .iter()
            .enumerate()
            .map(|(actor, ticket)| {
                let seat = Seat {
                    server: scenario.server.clone(),
                    match_id: created.match_id,
                    actor,
                    ticket: from_hex(ticket).unwrap_or_default(),
                    link: scenario.link,
                    paced: scenario.paced,
                    seed: rng.next_u64(),
                    reconnect_every: scenario.reconnect_every,
                    away: scenario.away,
                    layout: layout.clone(),
                    shared: shared.clone(),
                };
                tokio::spawn(bot::play(seat, deadline))
            })
            .collect();
        let mut finished = false;
        for seat in seats {
            let stats = seat.await.map_err(|error| error.to_string())?;
            finished |= stats.round_finished;
            totals.add(stats);
        }
        if !finished {
            break;
        }
        totals.rounds_finished += 1;
    }
    Ok(totals)
}

/// A minimal HTTP/1.1 exchange with `Connection: close`.
pub async fn http(server: &str, method: &str, path: &str) -> Result<(u16, String), String> {
    let mut stream = TcpStream::connect(server)
        .await
        .map_err(|error| format!("cannot reach {server}: {error}"))?;
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {server}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|error| error.to_string())?;
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .await
        .map_err(|error| error.to_string())?;
    let status = response
        .get(9..12)
        .and_then(|code| code.parse().ok())
        .ok_or("malformed HTTP response")?;
    let body = response
        .split_once("\r\n\r\n")
        .map_or("", |(_, body)| body)
        .to_owned();
    Ok((status, body))
}

/// Nearest-rank p50, p99 and maximum; zeros when empty.
fn percentiles(mut values: Vec<u32>) -> [u32; 3] {
    if values.is_empty() {
        return [0; 3];
    }
    values.sort_unstable();
    let at = |fraction: f64| {
        let rank = ((fraction * values.len() as f64).ceil() as usize).clamp(1, values.len());
        values[rank - 1]
    };
    [at(0.5), at(0.99), at(1.0)]
}

fn parse_metrics(text: &str) -> ServerMetrics {
    let value = |name: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(name)?.strip_prefix(' ')?.parse().ok())
            .unwrap_or(0)
    };
    ServerMetrics {
        ticks_total: value("arena_ticks_total"),
        tick_overruns_total: value("arena_tick_overruns_total"),
        tick_max_microseconds: value("arena_tick_max_microseconds"),
        frames_dropped_total: value("arena_frames_dropped_total"),
        inputs_overflow_total: value("arena_inputs_overflow_total"),
        inputs_stale_total: value("arena_inputs_stale_total"),
        inputs_folded_total: value("arena_inputs_folded_total"),
        inputs_starved_total: value("arena_inputs_starved_total"),
    }
}

/// User plus system CPU seconds of a Linux process (100 clock ticks/s).
fn cpu_seconds(pid: u32) -> Option<f64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let fields: Vec<&str> = stat.rsplit_once(')')?.1.split_whitespace().collect();
    let user: f64 = fields.get(11)?.parse().ok()?;
    let system: f64 = fields.get(12)?.parse().ok()?;
    Some((user + system) / 100.0)
}

pub(crate) fn from_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) || !text.is_ascii() {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).ok())
        .collect()
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.queue {
            Some(queue) => writeln!(
                f,
                "scenario       {} bots queueing for {queue} for {:.1} s",
                self.players, self.seconds
            )?,
            None => writeln!(
                f,
                "scenario       {} matches × {}v{} = {} bots against {} for {:.1} s",
                self.matches,
                self.team_size,
                self.team_size,
                self.players,
                self.server,
                self.seconds
            )?,
        }
        let mut link_notes = String::new();
        if self.loss_pct > 0.0 {
            link_notes.push_str(&format!(", loss {:.1}% (TCP HoL)", self.loss_pct));
        }
        if self.clump_ms > 0.0 {
            link_notes.push_str(&format!(", clump {:.0} ms", self.clump_ms));
        }
        if !self.paced {
            link_notes.push_str(", unpaced");
        }
        if let Some(every) = self.reconnect_every_s {
            link_notes.push_str(&format!(", rejoin every {every:.0} s"));
        }
        writeln!(
            f,
            "link           base RTT {:.0} ms, jitter up to {:.0} ms per direction{link_notes}",
            self.rtt_ms, self.jitter_ms
        )?;
        writeln!(
            f,
            "rounds         {} started, {} finished",
            self.rounds_started, self.rounds_finished
        )?;
        writeln!(f, "rtt (ping)     {}", self.rtt)?;
        writeln!(f, "input → ack    {}", self.input_latency)?;
        writeln!(f, "snapshot gap   {}", self.snapshot_gap)?;
        writeln!(
            f,
            "traffic        {} snapshots, {} inputs, {:.1} kbit/s in per bot (payload only)",
            self.snapshots, self.inputs, self.kbit_per_s_in_per_player
        )?;
        writeln!(
            f,
            "consistency    {} cross-seat hash checks, {} mismatches, {} wrong round results",
            self.hash_checks, self.hash_mismatches, self.wrong_results
        )?;
        writeln!(
            f,
            "failures       errors {:?}, unexpected closes {} {:?}",
            self.errors, self.unexpected_closes, self.lost
        )?;
        writeln!(
            f,
            "recovery       {}/{} rejoins succeeded ({} found the round already over), {} snapshots showed someone disconnected",
            self.rejoins_succeeded,
            self.rejoins_attempted,
            self.rejoins_too_late,
            self.snapshots_showing_disconnected
        )?;
        let per_thousand = if self.predicted_inputs == 0 {
            0.0
        } else {
            self.corrections as f64 * 1000.0 / self.predicted_inputs as f64
        };
        let [p50, p99, max] = self.correction_mm;
        writeln!(
            f,
            "prediction     {} corrections over {} predicted inputs ({per_thousand:.1} per 1000), size p50 {p50} mm  p99 {p99} mm  max {max} mm",
            self.corrections, self.predicted_inputs
        )?;
        if let Some(server) = &self.server_metrics {
            writeln!(
                f,
                "server         {} ticks, {} overruns, slowest tick {} µs, {} frames dropped, {} inputs overflowed, {} folded, {} starved",
                server.ticks_total,
                server.tick_overruns_total,
                server.tick_max_microseconds,
                server.frames_dropped_total,
                server.inputs_overflow_total,
                server.inputs_folded_total,
                server.inputs_starved_total
            )?;
        }
        if let Some(cores) = self.server_cpu_cores {
            writeln!(f, "server cpu     {cores:.2} cores on average")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_cross_checks_count_only_comparisons_and_catch_divergence() {
        let shared = Shared::default();
        shared.check_hash(1, 10, 99);
        shared.check_hash(1, 10, 99);
        shared.check_hash(2, 10, 5);
        shared.check_hash(1, 10, 98);
        assert_eq!(shared.checks.load(Ordering::Relaxed), 2);
        assert_eq!(shared.mismatches.load(Ordering::Relaxed), 1);
        for tick in 0..1_000 {
            shared.check_hash(3, tick, tick);
        }
        assert_eq!(shared.hashes.lock().unwrap()[&3].len(), 600);
    }

    #[test]
    fn metrics_text_is_parsed_by_exact_name() {
        let text = "# HELP arena_ticks_total x\narena_ticks_total 12\narena_tick_overruns_total 3\narena_tick_max_microseconds 450\narena_inputs_folded_total 7\narena_inputs_starved_total 11\n";
        let metrics = parse_metrics(text);
        assert_eq!(
            (
                metrics.ticks_total,
                metrics.tick_overruns_total,
                metrics.tick_max_microseconds,
                metrics.inputs_folded_total,
                metrics.inputs_starved_total,
            ),
            (12, 3, 450, 7, 11)
        );
        assert_eq!(metrics.frames_dropped_total, 0);
        assert_eq!(from_hex("0aff"), Some(vec![10, 255]));
        assert_eq!(from_hex("0"), None);
    }
}
