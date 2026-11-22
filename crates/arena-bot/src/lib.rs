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

pub mod bandwidth;
pub mod bot;
pub mod fairness;
pub mod link;
pub mod queue;
pub mod stats;

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use arena_headless::telemetry::{CombatMetrics, DurationSummary, OutcomeSummary};
use arena_protocol::{FighterScore, Snapshot, WireEvent};
use arena_sim::TICKS_PER_SECOND;
use arena_sim::arena::RoundResult;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::Instant;

use bandwidth::{BandwidthReport, BandwidthStats};
use bot::{Seat, SeatStats};
use fairness::{FairnessStats, TargetingFairnessReport};
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
    /// Remote-fighter view delay (ms), matching browser interpolation (default 100 ms).
    pub interp_delay_ms: u64,
    /// Use browser-parity adaptive interpolation delay (`InterpDelay`) instead of fixed `interp_delay_ms`.
    pub adaptive_interp: bool,
    pub reconnect_every: Option<Duration>,
    /// How long a dropped seat stays away before rejoining.
    pub away: Duration,
    pub seed: u64,
    /// Linux only: measure this process's CPU, memory and context switches over the run.
    pub server_pid: Option<u32>,
    /// Queue through a front door instead of creating matches.
    pub queue: Option<queue::QueueScenario>,
}

#[derive(Debug, Default)]
struct MatchHistory {
    /// `tick -> (state_hash, public_events_recorded)`.
    /// Tracking `public_events_recorded` separately from the hash prevents an
    /// empty-events reconnect `attach()` snapshot arriving ahead of a peer's
    /// regular tick snapshot from silently dropping that tick's public events.
    hashes: BTreeMap<u64, (u64, bool)>,
    /// Highest tick evicted from `hashes`; any tick at or below this was
    /// already seen by another seat and must not be re-counted.
    evicted_up_to: Option<u64>,
}

#[derive(Debug, Default)]
struct SharedState {
    matches: HashMap<u64, MatchHistory>,
    /// Public combat events deduplicated per `(match_id, snapshot.tick)`.
    public_combat: CombatMetrics,
    /// Finished match outcomes deduplicated per `match_id`: `(result, duration_ticks, tick_rate)`.
    finished_matches: HashMap<u64, (RoundResult, u64, u32)>,
    /// Final authoritative scoreboards observed per `match_id`, cross-checked across seats.
    scoreboards: HashMap<u64, Option<Vec<FighterScore>>>,
}

/// State hashes and deduplicated combat telemetry seen per match and tick,
/// catching any divergence between what different players are told while
/// counting public combat events and round outcomes exactly once per match.
#[derive(Debug, Default)]
pub struct Shared {
    state: Mutex<SharedState>,
    checks: AtomicU64,
    mismatches: AtomicU64,
}
impl Shared {
    fn record_hash_and_check_events_locked(
        &self,
        state: &mut SharedState,
        match_id: u64,
        tick: u64,
        hash: u64,
        has_public_events: bool,
    ) -> bool {
        let entry = state.matches.entry(match_id).or_default();
        if entry.evicted_up_to.is_some_and(|cut| tick <= cut) {
            return false;
        }
        match entry.hashes.get_mut(&tick) {
            Some((seen, recorded)) => {
                self.checks.fetch_add(1, Ordering::Relaxed);
                if *seen != hash {
                    self.mismatches.fetch_add(1, Ordering::Relaxed);
                }
                if has_public_events && !*recorded {
                    *recorded = true;
                    true
                } else {
                    false
                }
            }
            None => {
                entry.hashes.insert(tick, (hash, has_public_events));
                // About 20 s of history covers any lagging seat.
                while entry.hashes.len() > 600 {
                    if let Some((oldest, _)) = entry.hashes.pop_first() {
                        entry.evicted_up_to =
                            Some(entry.evicted_up_to.map_or(oldest, |prev| prev.max(oldest)));
                    }
                }
                has_public_events
            }
        }
    }

    pub fn check_hash(&self, match_id: u64, tick: u64, hash: u64) {
        let mut state = self.state.lock().expect("shared state lock");
        let _ = self.record_hash_and_check_events_locked(&mut state, match_id, tick, hash, false);
    }

    /// Cross-checks `snapshot.hash` across seats and deduplicates public
    /// combat events and finished-match outcomes per `match_id`.
    pub fn observe_snapshot(&self, match_id: u64, tick_rate: u32, snapshot: &Snapshot) {
        let mut state = self.state.lock().expect("shared state lock");
        if snapshot.tick > 0 {
            let has_public_events = snapshot.events.iter().any(|tick_event| {
                !matches!(
                    tick_event.event,
                    WireEvent::Finished(_) | WireEvent::Rejected(_)
                )
            });
            if self.record_hash_and_check_events_locked(
                &mut state,
                match_id,
                snapshot.tick,
                snapshot.hash,
                has_public_events,
            ) {
                for tick_event in &snapshot.events {
                    match tick_event.event {
                        WireEvent::Jumped { .. } => state.public_combat.record_jump(),
                        WireEvent::CastStarted { spell, .. } => {
                            state.public_combat.record_cast_started(spell.into());
                        }
                        WireEvent::CastCompleted { spell, .. } => {
                            state.public_combat.record_cast_completed(spell.into());
                        }
                        WireEvent::CastInterrupted { spell, reason, .. } => {
                            state
                                .public_combat
                                .record_cast_interrupted(spell.into(), reason.into());
                        }
                        WireEvent::Hit { actor, damage, .. } => {
                            state.public_combat.record_hit(usize::from(actor), damage);
                        }
                        WireEvent::Healed { actor, amount, .. } => {
                            state.public_combat.record_heal(usize::from(actor), amount);
                        }
                        // `Finished` is tracked per `match_id` below; `Rejected` is private per seat.
                        WireEvent::Finished(_) | WireEvent::Rejected(_) => {}
                    }
                }
            }
        }
        if let Some(result) = snapshot.result {
            state.finished_matches.entry(match_id).or_insert((
                RoundResult::from(result),
                snapshot.tick,
                tick_rate.max(1),
            ));
            match state.scoreboards.get(&match_id) {
                Some(seen) => {
                    self.checks.fetch_add(1, Ordering::Relaxed);
                    if seen != &snapshot.scoreboard {
                        self.mismatches.fetch_add(1, Ordering::Relaxed);
                    }
                }
                None => {
                    state
                        .scoreboards
                        .insert(match_id, snapshot.scoreboard.clone());
                }
            }
        }
    }

    /// Returns deduplicated match outcomes, round durations, and public combat counters.
    pub fn telemetry_summary(&self) -> (OutcomeSummary, DurationSummary, CombatMetrics) {
        let state = self.state.lock().expect("shared state lock");
        let outcomes = OutcomeSummary::from_results(
            state
                .finished_matches
                .values()
                .map(|(result, _, _)| *result),
        );
        let tick_rate = state
            .finished_matches
            .values()
            .next()
            .map_or(TICKS_PER_SECOND, |(_, _, rate)| u64::from(*rate));
        let durations: Vec<u64> = state
            .finished_matches
            .values()
            .map(|(_, ticks, _)| *ticks)
            .collect();
        let duration = DurationSummary::from_ticks(durations, tick_rate);
        (outcomes, duration, state.public_combat.clone())
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
    pub tick_microseconds_total: u64,
    pub tick_mean_microseconds: f64,
    pub frames_dropped_total: u64,
    pub inputs_overflow_total: u64,
    pub inputs_stale_total: u64,
    pub inputs_folded_total: u64,
    pub inputs_starved_total: u64,
    pub matches_finished_won: u64,
    pub matches_finished_draw: u64,
    pub matches_finished_time_limit_draw: u64,
}

/// Linux `/proc/<pid>` + `/metrics` breakdown of `arena-server` CPU, memory,
/// and scheduler wake-up activity during the scenario window.
#[derive(Clone, Debug, Default, Serialize)]
pub struct ServerProfile {
    /// Total process CPU cores in use (`cpu_user_cores + cpu_sys_cores`).
    pub cpu_total_cores: f64,
    /// Userspace CPU cores (Tokio scheduling, WebSocket framing, decode, match ticks).
    pub cpu_user_cores: f64,
    /// Kernel CPU cores (TCP/loopback reads & writes, epoll, futex wake-ups).
    pub cpu_sys_cores: f64,
    /// Match-task tick compute cores (`delta(arena_tick_microseconds_total) / wall_us`).
    pub match_tick_cpu_cores: f64,
    /// Non-tick userspace CPU cores (`(cpu_user_cores - match_tick_cpu_cores).max(0)`).
    pub non_tick_user_cores: f64,
    /// Peak resident set size (`VmHWM`) in MiB.
    pub peak_rss_mib: f64,
    /// Current OS thread count (`Threads`) at end of run.
    pub threads: u32,
    /// Voluntary context switches per second across all server threads.
    pub voluntary_ctxt_switches_per_s: f64,
    /// Non-voluntary context switches per second across all server threads.
    pub nonvoluntary_ctxt_switches_per_s: f64,
    /// Total server CPU microseconds divided by total snapshots delivered in this run.
    pub cpu_us_per_snapshot_sent: f64,
    /// Voluntary server context switches divided by total snapshots delivered in this run.
    pub voluntary_switches_per_snapshot: f64,
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
    pub interp_delay_ms: u64,
    pub adaptive_interp: bool,
    /// Sampled effective interpolation delay in ms when `--adaptive-interp` is enabled: `[p50, p99, max]`.
    pub effective_interp_delay_ms: [u32; 3],
    pub paced: bool,
    pub reconnect_every_s: Option<f64>,
    pub rounds_started: u64,
    pub rounds_finished: u64,
    /// Deduplicated finished-match outcomes and win/draw rates.
    pub outcomes: OutcomeSummary,
    /// Distribution of completed match durations in ticks and seconds.
    pub round_duration: DurationSummary,
    /// Aggregated combat telemetry (LOS blocks, hits, damage, heals, casts, interrupts).
    pub combat: CombatMetrics,
    /// Targeting-fairness / lag-compensation disagreement metrics.
    pub targeting_fairness: TargetingFairnessReport,
    /// Full 30 Hz vs lossless delta & 20 Hz / 15 Hz snapshot bandwidth analysis.
    pub bandwidth: BandwidthReport,
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
    /// Detailed Linux process CPU/memory/context-switch breakdown when `--server-pid` is set.
    pub server_profile: Option<ServerProfile>,
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
    interp_delay_ms: Vec<u32>,
    rejections: CombatMetrics,
    fairness: FairnessStats,
    bandwidth: BandwidthStats,
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
        self.interp_delay_ms.extend(seat.interp_delay_ms);
        self.rejections.merge(&seat.rejections);
        self.fairness.merge(seat.fairness);
        self.bandwidth.merge(seat.bandwidth);
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
        self.interp_delay_ms.extend(other.interp_delay_ms);
        self.rejections.merge(&other.rejections);
        self.fairness.merge(other.fairness);
        self.bandwidth.merge(other.bandwidth);
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
    let metrics_start = fetch_metrics(&scenario.server).await;
    let started = Instant::now();
    let deadline = started + scenario.duration;
    let proc_start = scenario.server_pid.and_then(sample_proc);
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
    let proc_end = scenario.server_pid.and_then(sample_proc);
    let server_metrics = fetch_metrics(&scenario.server).await;
    let delta_tick_us =
        metrics_start
            .as_ref()
            .zip(server_metrics.as_ref())
            .map_or(0, |(start, end)| {
                end.tick_microseconds_total
                    .saturating_sub(start.tick_microseconds_total)
            });
    let server_profile = proc_start.zip(proc_end).map(|(start, end)| {
        build_server_profile(start, end, delta_tick_us, totals.snapshots, seconds)
    });
    let server_cpu_cores = server_profile
        .as_ref()
        .map(|profile| profile.cpu_total_cores);
    let players = match &scenario.queue {
        Some(queue) => queue.bots,
        None => scenario.matches * scenario.team_size * 2,
    };
    let (outcomes, round_duration, mut combat) = shared.telemetry_summary();
    combat.merge(&totals.rejections);
    let targeting_fairness = totals.fairness.summarize();
    let bandwidth = totals.bandwidth.summarize(TICKS_PER_SECOND as u32);
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
        interp_delay_ms: scenario.interp_delay_ms,
        adaptive_interp: scenario.adaptive_interp,
        effective_interp_delay_ms: if scenario.adaptive_interp {
            percentiles(totals.interp_delay_ms)
        } else {
            let fixed = u32::try_from(scenario.interp_delay_ms).unwrap_or(u32::MAX);
            [fixed; 3]
        },
        paced: scenario.paced,
        reconnect_every_s: scenario.reconnect_every.map(|every| every.as_secs_f64()),
        rounds_started: totals.rounds_started,
        rounds_finished: totals.rounds_finished,
        outcomes,
        round_duration,
        combat,
        targeting_fairness,
        bandwidth,
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
        server_profile,
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
                    interp_delay_ms: scenario.interp_delay_ms,
                    adaptive_interp: scenario.adaptive_interp,
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
pub(crate) fn percentiles(mut values: Vec<u32>) -> [u32; 3] {
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
    let ticks_total = value("arena_ticks_total");
    let tick_microseconds_total = value("arena_tick_microseconds_total");
    let tick_mean_microseconds = if ticks_total == 0 {
        0.0
    } else {
        tick_microseconds_total as f64 / ticks_total as f64
    };
    ServerMetrics {
        ticks_total,
        tick_overruns_total: value("arena_tick_overruns_total"),
        tick_max_microseconds: value("arena_tick_max_microseconds"),
        tick_microseconds_total,
        tick_mean_microseconds,
        frames_dropped_total: value("arena_frames_dropped_total"),
        inputs_overflow_total: value("arena_inputs_overflow_total"),
        inputs_stale_total: value("arena_inputs_stale_total"),
        inputs_folded_total: value("arena_inputs_folded_total"),
        inputs_starved_total: value("arena_inputs_starved_total"),
        matches_finished_won: value("arena_matches_finished_total{result=\"won\"}"),
        matches_finished_draw: value("arena_matches_finished_total{result=\"draw\"}"),
        matches_finished_time_limit_draw: value(
            "arena_matches_finished_total{result=\"time_limit_draw\"}",
        ),
    }
}

async fn fetch_metrics(server: &str) -> Option<ServerMetrics> {
    http(server, "GET", "/metrics")
        .await
        .ok()
        .filter(|(status, _)| *status == 200)
        .map(|(_, body)| parse_metrics(&body))
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct ProcSample {
    user_s: f64,
    sys_s: f64,
    peak_rss_kib: u64,
    threads: u32,
    voluntary_ctxt_switches: u64,
    nonvoluntary_ctxt_switches: u64,
}

fn parse_proc_stat(stat: &str) -> Option<(f64, f64)> {
    let fields: Vec<&str> = stat.rsplit_once(')')?.1.split_whitespace().collect();
    let user: f64 = fields.get(11)?.parse().ok()?;
    let system: f64 = fields.get(12)?.parse().ok()?;
    Some((user / 100.0, system / 100.0))
}

fn parse_proc_status_field(status: &str, key: &str) -> Option<u64> {
    status.lines().find_map(|line| {
        line.strip_prefix(key)?
            .split_whitespace()
            .next()?
            .parse()
            .ok()
    })
}

fn sample_proc(pid: u32) -> Option<ProcSample> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let (user_s, sys_s) = parse_proc_stat(&stat)?;
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).unwrap_or_default();
    let peak_rss_kib = parse_proc_status_field(&status, "VmHWM:").unwrap_or(0);
    let threads = parse_proc_status_field(&status, "Threads:")
        .and_then(|v| u32::try_from(v).ok())
        .unwrap_or(1);
    let (voluntary_ctxt_switches, nonvoluntary_ctxt_switches) = sum_task_switches(pid)
        .unwrap_or_else(|| {
            (
                parse_proc_status_field(&status, "voluntary_ctxt_switches:").unwrap_or(0),
                parse_proc_status_field(&status, "nonvoluntary_ctxt_switches:").unwrap_or(0),
            )
        });
    Some(ProcSample {
        user_s,
        sys_s,
        peak_rss_kib,
        threads,
        voluntary_ctxt_switches,
        nonvoluntary_ctxt_switches,
    })
}

fn sum_task_switches(pid: u32) -> Option<(u64, u64)> {
    let entries = std::fs::read_dir(format!("/proc/{pid}/task")).ok()?;
    let (mut vol, mut nonvol, mut saw_any) = (0u64, 0u64, false);
    for entry in entries.flatten() {
        let Ok(text) = std::fs::read_to_string(entry.path().join("status")) else {
            continue;
        };
        saw_any = true;
        vol = vol.saturating_add(
            parse_proc_status_field(&text, "voluntary_ctxt_switches:").unwrap_or(0),
        );
        nonvol = nonvol.saturating_add(
            parse_proc_status_field(&text, "nonvoluntary_ctxt_switches:").unwrap_or(0),
        );
    }
    saw_any.then_some((vol, nonvol))
}

fn build_server_profile(
    start: ProcSample,
    end: ProcSample,
    delta_tick_us: u64,
    snapshots: u64,
    seconds: f64,
) -> ServerProfile {
    let wall = seconds.max(1e-9);
    let cpu_user_cores = (end.user_s - start.user_s).max(0.0) / wall;
    let cpu_sys_cores = (end.sys_s - start.sys_s).max(0.0) / wall;
    let cpu_total_cores = cpu_user_cores + cpu_sys_cores;
    let match_tick_cpu_cores = (delta_tick_us as f64 / 1_000_000.0) / wall;
    let non_tick_user_cores = (cpu_user_cores - match_tick_cpu_cores).max(0.0);
    let vol_delta = end
        .voluntary_ctxt_switches
        .saturating_sub(start.voluntary_ctxt_switches);
    let nonvol_delta = end
        .nonvoluntary_ctxt_switches
        .saturating_sub(start.nonvoluntary_ctxt_switches);
    let snaps = snapshots.max(1) as f64;
    ServerProfile {
        cpu_total_cores,
        cpu_user_cores,
        cpu_sys_cores,
        match_tick_cpu_cores,
        non_tick_user_cores,
        peak_rss_mib: end.peak_rss_kib as f64 / 1024.0,
        threads: end.threads,
        voluntary_ctxt_switches_per_s: vol_delta as f64 / wall,
        nonvoluntary_ctxt_switches_per_s: nonvol_delta as f64 / wall,
        cpu_us_per_snapshot_sent: if snapshots == 0 {
            0.0
        } else {
            (cpu_total_cores * wall * 1_000_000.0) / snaps
        },
        voluntary_switches_per_snapshot: if snapshots == 0 {
            0.0
        } else {
            vol_delta as f64 / snaps
        },
    }
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
        let interp_note = if self.adaptive_interp {
            let [p50, p99, max] = self.effective_interp_delay_ms;
            format!("adaptive interp p50 {p50} p99 {p99} max {max} ms")
        } else {
            format!("interp delay {} ms", self.interp_delay_ms)
        };
        writeln!(
            f,
            "link           base RTT {:.0} ms, jitter up to {:.0} ms per direction, {interp_note}{link_notes}",
            self.rtt_ms, self.jitter_ms
        )?;
        writeln!(
            f,
            "rounds         {} started, {} finished",
            self.rounds_started, self.rounds_finished
        )?;
        writeln!(
            f,
            "outcomes       {} resolved: team0 {} ({:.1}%), team1 {} ({:.1}%), draws {} ({:.1}%)",
            self.outcomes.matches_finished,
            self.outcomes.team0_wins,
            self.outcomes.team0_win_rate * 100.0,
            self.outcomes.team1_wins,
            self.outcomes.team1_win_rate * 100.0,
            self.outcomes.total_draws,
            self.outcomes.draw_rate * 100.0
        )?;
        if self.round_duration.count > 0 {
            writeln!(
                f,
                "duration       mean {:.1} ticks ({:.2} s)  p50 {} ({:.2} s)  max {} ({:.2} s)",
                self.round_duration.mean_ticks,
                self.round_duration.mean_s,
                self.round_duration.p50_ticks,
                self.round_duration.p50_s,
                self.round_duration.max_ticks,
                self.round_duration.max_s
            )?;
        }
        writeln!(
            f,
            "combat         {} hits ({} dmg), {} heals ({} hp), {} LOS blocks, {} interrupts landed",
            self.combat.hits,
            self.combat.total_damage,
            self.combat.heals,
            self.combat.total_healing,
            self.combat.los_blocks,
            self.combat.interrupts_landed
        )?;
        let tf = &self.targeting_fairness;
        let [de_p50, de_p99, de_max] = tf.distance_error_mm;
        writeln!(
            f,
            "fairness       {} casts resolved ({} reached geom, {} accepted, {} lag-comped, {} folded), OOR disagree {}/{} ({:.2}%), LoS disagree {}/{} ({:.2}%), dist err p50 {de_p50} p99 {de_p99} max {de_max} mm",
            tf.casts_resolved,
            tf.casts_reached_geometry,
            tf.casts_accepted,
            tf.casts_saved_by_lag_comp,
            tf.casts_folded_or_skipped,
            tf.disagreed_out_of_range,
            tf.casts_reached_geometry_in_range,
            tf.out_of_range_disagreement_rate * 100.0,
            tf.disagreed_los,
            tf.casts_reached_geometry_has_los,
            tf.los_disagreement_rate * 100.0,
        )?;
        let bw = &self.bandwidth;
        writeln!(
            f,
            "bandwidth      30Hz full {:.1} kbit/s ({:.1} B/snap), 30Hz delta {:.1} (-{:.1}%), 20Hz full {:.1} / delta {:.1}, 15Hz full {:.1} / delta {:.1}",
            bw.full_30hz_kbit_per_s_per_player,
            bw.full_30hz_mean_bytes,
            bw.delta_30hz_kbit_per_s_per_player,
            bw.delta_30hz_savings_pct,
            bw.full_20hz_kbit_per_s_per_player,
            bw.delta_20hz_kbit_per_s_per_player,
            bw.full_15hz_kbit_per_s_per_player,
            bw.delta_15hz_kbit_per_s_per_player,
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
                "server         {} ticks, {} overruns, mean tick {:.1} µs, slowest tick {} µs, {} frames dropped, {} inputs overflowed, {} folded, {} starved",
                server.ticks_total,
                server.tick_overruns_total,
                server.tick_mean_microseconds,
                server.tick_max_microseconds,
                server.frames_dropped_total,
                server.inputs_overflow_total,
                server.inputs_folded_total,
                server.inputs_starved_total
            )?;
        }
        if let Some(profile) = &self.server_profile {
            writeln!(
                f,
                "server cpu     {:.2} cores (user {:.2} = tick {:.2} + non-tick {:.2}, sys {:.2}), peak RSS {:.1} MiB, {} threads, {:.0} vol-cs/s ({:.2}/snap), {:.1} µs/snap",
                profile.cpu_total_cores,
                profile.cpu_user_cores,
                profile.match_tick_cpu_cores,
                profile.non_tick_user_cores,
                profile.cpu_sys_cores,
                profile.peak_rss_mib,
                profile.threads,
                profile.voluntary_ctxt_switches_per_s,
                profile.voluntary_switches_per_snapshot,
                profile.cpu_us_per_snapshot_sent,
            )?;
        } else if let Some(cores) = self.server_cpu_cores {
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
        assert_eq!(shared.state.lock().unwrap().matches[&3].hashes.len(), 600);
        // An already-evicted tick from a lagging seat must not be re-inserted or miscounted.
        shared.check_hash(3, 0, 999);
        assert_eq!(shared.mismatches.load(Ordering::Relaxed), 1);
        assert_eq!(shared.state.lock().unwrap().matches[&3].hashes.len(), 600);
    }

    #[test]
    fn observe_snapshot_deduplicates_public_events_and_outcomes_across_seats() {
        use arena_protocol::{
            TickEvent, WireInterruptReason, WireRejection, WireResult, WireSpell,
        };
        let shared = Shared::default();
        let snap = Snapshot {
            tick: 45,
            ack: 1,
            queued: 0,
            hash: 0x1234,
            result: Some(WireResult::TimeLimitDraw),
            fighters: Vec::new(),
            scoreboard: Some(vec![FighterScore::default(); 2]),
            events: vec![
                TickEvent {
                    tick: 44,
                    event: WireEvent::Hit {
                        actor: 0,
                        target: 1,
                        damage: 25,
                    },
                },
                TickEvent {
                    tick: 44,
                    event: WireEvent::CastInterrupted {
                        actor: 1,
                        spell: WireSpell::RiftBolt,
                        reason: WireInterruptReason::LineOfSight,
                        by: None,
                    },
                },
                TickEvent {
                    tick: 44,
                    event: WireEvent::Rejected(WireRejection::LineOfSight),
                },
            ],
        };
        // Both seats observe the same tick and finish snapshot.
        shared.observe_snapshot(7, 30, &snap);
        shared.observe_snapshot(7, 30, &snap);
        assert_eq!(shared.checks.load(Ordering::Relaxed), 2);
        assert_eq!(shared.mismatches.load(Ordering::Relaxed), 0);

        // Divergent scoreboard between seats for the same finished match counts as a mismatch.
        let mut divergent = snap.clone();
        divergent.scoreboard = Some(vec![
            FighterScore {
                damage_done: 99,
                ..FighterScore::default()
            },
            FighterScore::default(),
        ]);
        shared.observe_snapshot(7, 30, &divergent);
        assert_eq!(shared.mismatches.load(Ordering::Relaxed), 1);

        let (outcomes, duration, combat) = shared.telemetry_summary();
        assert_eq!(outcomes.matches_finished, 1);
        assert_eq!(outcomes.time_limit_draws, 1);
        assert!((outcomes.draw_rate - 1.0).abs() < 1e-12);
        assert_eq!(duration.count, 1);
        assert_eq!(duration.p50_ticks, 45);
        assert_eq!(combat.hits, 1);
        assert_eq!(combat.total_damage, 25);
        assert_eq!(combat.los_cast_interrupts, 1);
        // Private rejections are counted per seat, not in Shared's public events.
        assert_eq!(combat.los_rejections, 0);
        assert_eq!(combat.los_blocks, 1);

        // Reconnect attach() race: an empty-events snapshot arriving ahead of regular
        // seat snapshots for tick 46 must not drop tick 46's public combat events.
        let attach_snap = Snapshot {
            tick: 46,
            ack: 1,
            queued: 0,
            hash: 0x5678,
            result: None,
            fighters: Vec::new(),
            scoreboard: None,
            events: Vec::new(),
        };
        let regular_snap = Snapshot {
            tick: 46,
            ack: 2,
            queued: 0,
            hash: 0x5678,
            result: None,
            fighters: Vec::new(),
            scoreboard: None,
            events: vec![TickEvent {
                tick: 45,
                event: WireEvent::Hit {
                    actor: 1,
                    target: 0,
                    damage: 30,
                },
            }],
        };
        shared.observe_snapshot(8, 30, &attach_snap);
        shared.observe_snapshot(8, 30, &regular_snap);
        shared.observe_snapshot(8, 30, &regular_snap);
        assert_eq!(shared.mismatches.load(Ordering::Relaxed), 1);
        let (_, _, combat_after_attach) = shared.telemetry_summary();
        assert_eq!(combat_after_attach.hits, 2);
        assert_eq!(combat_after_attach.total_damage, 55);

        // Evicted ticks from lagging seats must not double-count public combat events.
        for tick in 1..=650 {
            let s = Snapshot {
                tick,
                ack: 0,
                queued: 0,
                hash: tick,
                result: None,
                fighters: Vec::new(),
                scoreboard: None,
                events: Vec::new(),
            };
            shared.observe_snapshot(9, 30, &s);
        }
        let lagging = Snapshot {
            tick: 1,
            ack: 0,
            queued: 0,
            hash: 1,
            result: None,
            fighters: Vec::new(),
            scoreboard: None,
            events: vec![TickEvent {
                tick: 0,
                event: WireEvent::Hit {
                    actor: 0,
                    target: 1,
                    damage: 99,
                },
            }],
        };
        shared.observe_snapshot(9, 30, &lagging);
        let (_, _, combat_after_eviction) = shared.telemetry_summary();
        assert_eq!(combat_after_eviction.hits, 2);
        assert_eq!(combat_after_eviction.total_damage, 55);
    }

    #[test]
    fn metrics_text_is_parsed_by_exact_name() {
        let text = "# HELP arena_ticks_total x\narena_ticks_total 12\narena_tick_overruns_total 3\narena_tick_max_microseconds 450\narena_tick_microseconds_total 600\narena_inputs_folded_total 7\narena_inputs_starved_total 11\narena_matches_finished_total{result=\"won\"} 4\narena_matches_finished_total{result=\"draw\"} 1\narena_matches_finished_total{result=\"time_limit_draw\"} 2\n";
        let metrics = parse_metrics(text);
        assert_eq!(
            (
                metrics.ticks_total,
                metrics.tick_overruns_total,
                metrics.tick_max_microseconds,
                metrics.tick_microseconds_total,
                metrics.inputs_folded_total,
                metrics.inputs_starved_total,
                metrics.matches_finished_won,
                metrics.matches_finished_draw,
                metrics.matches_finished_time_limit_draw,
            ),
            (12, 3, 450, 600, 7, 11, 4, 1, 2)
        );
        assert!((metrics.tick_mean_microseconds - 50.0).abs() < 1e-12);
        assert_eq!(metrics.frames_dropped_total, 0);
        // Zero ticks yields finite 0.0 mean instead of NaN.
        let empty = parse_metrics("");
        assert_eq!(empty.tick_mean_microseconds, 0.0);
        assert_eq!(from_hex("0aff"), Some(vec![10, 255]));
        assert_eq!(from_hex("0"), None);
    }

    #[test]
    fn proc_stat_and_status_parsers_extract_cpu_memory_and_context_switches() {
        let stat = "1234 (arena-server) S 1 2 3 4 5 6 7 8 9 10 250 150 0 0 20 0 8 0";
        assert_eq!(parse_proc_stat(stat), Some((2.5, 1.5)));
        assert_eq!(parse_proc_stat("malformed"), None);

        let status = "Name:\tarena-server\nThreads:\t9\nVmHWM:\t   32768 kB\nvoluntary_ctxt_switches:\t4200\nnonvoluntary_ctxt_switches:\t31\n";
        assert_eq!(parse_proc_status_field(status, "VmHWM:"), Some(32768));
        assert_eq!(parse_proc_status_field(status, "Threads:"), Some(9));
        assert_eq!(
            parse_proc_status_field(status, "voluntary_ctxt_switches:"),
            Some(4200)
        );
        assert_eq!(
            parse_proc_status_field(status, "nonvoluntary_ctxt_switches:"),
            Some(31)
        );
        assert_eq!(parse_proc_status_field(status, "Missing:"), None);

        let start = ProcSample {
            user_s: 1.0,
            sys_s: 0.5,
            peak_rss_kib: 10 * 1024,
            threads: 9,
            voluntary_ctxt_switches: 1_000,
            nonvoluntary_ctxt_switches: 10,
        };
        let end = ProcSample {
            user_s: 3.0,
            sys_s: 1.5,
            peak_rss_kib: 20 * 1024,
            threads: 9,
            voluntary_ctxt_switches: 6_000,
            nonvoluntary_ctxt_switches: 60,
        };
        let profile = build_server_profile(start, end, 500_000, 10_000, 2.0);
        assert!((profile.cpu_user_cores - 1.0).abs() < 1e-9);
        assert!((profile.cpu_sys_cores - 0.5).abs() < 1e-9);
        assert!((profile.cpu_total_cores - 1.5).abs() < 1e-9);
        assert!((profile.match_tick_cpu_cores - 0.25).abs() < 1e-9);
        assert!((profile.non_tick_user_cores - 0.75).abs() < 1e-9);
        assert!((profile.peak_rss_mib - 20.0).abs() < 1e-9);
        assert!((profile.voluntary_ctxt_switches_per_s - 2500.0).abs() < 1e-9);
        assert!((profile.voluntary_switches_per_snapshot - 0.5).abs() < 1e-9);
        assert!((profile.cpu_us_per_snapshot_sent - 300.0).abs() < 1e-9);
    }
}
