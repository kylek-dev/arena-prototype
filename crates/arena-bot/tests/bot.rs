//! Bots against an in-process server at the real 30 Hz tick.

use std::net::SocketAddr;
use std::time::Duration;

use arena_bot::link::Link;
use arena_bot::{Scenario, run};
use arena_server::ticket::TicketKey;
use arena_server::{Config, start};

async fn scenario(rtt_ms: u64, jitter_ms: u64) -> (arena_server::Running, Scenario) {
    let mut config = Config::new(TicketKey::random());
    config.bind = SocketAddr::from(([127, 0, 0, 1], 0));
    config.dev = true;
    let running = start(config).await.unwrap();
    let scenario = Scenario {
        server: running.addr.to_string(),
        matches: 2,
        team_size: 1,
        duration: Duration::from_millis(1500),
        link: Link::round_trip(
            Duration::from_millis(rtt_ms),
            Duration::from_millis(jitter_ms),
        ),
        paced: true,
        interp_delay_ms: arena_bot::fairness::DEFAULT_INTERP_DELAY_MS,
        reconnect_every: None,
        away: Duration::from_millis(200),
        seed: 7,
        server_pid: Some(std::process::id()),
        queue: None,
    };
    (running, scenario)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn bots_play_consistently_through_simulated_latency() {
    let (running, scenario) = scenario(100, 20).await;
    let report = run(scenario).await.unwrap();
    println!("{report}");
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(report.unexpected_closes, 0);
    assert!(report.hash_checks > 30, "{}", report.hash_checks);
    assert_eq!(report.hash_mismatches, 0);
    // Pings cost at least the simulated round trip. Upper bounds are loose
    // so a busy CI runner cannot fail them; the research note records real
    // measurements.
    assert!(report.rtt.count > 0);
    assert!(report.rtt.p50_ms >= 100.0, "{}", report.rtt);
    assert!(report.rtt.p50_ms < 400.0, "{}", report.rtt);
    // An input waits a round trip plus up to one tick to be acknowledged.
    assert!(
        report.input_latency.p50_ms >= 100.0,
        "{}",
        report.input_latency
    );
    assert!(
        report.input_latency.p50_ms < 500.0,
        "{}",
        report.input_latency
    );
    assert!(report.snapshots > 0 && report.inputs > 0);
    // Bots predict their own movement like the browser does.
    assert!(report.predicted_inputs > 0);
    let server = report.server_metrics.as_ref().unwrap();
    assert!(server.ticks_total > 0);
    assert_eq!(server.frames_dropped_total, 0);
    running.shut_down(Duration::ZERO).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn bots_that_drop_out_are_seen_as_disconnected_and_rejoin() {
    let (running, mut scenario) = scenario(50, 0).await;
    scenario.duration = Duration::from_secs(2);
    scenario.reconnect_every = Some(Duration::from_millis(500));
    let report = run(scenario).await.unwrap();
    println!("{report}");
    assert!(report.rejoins_attempted > 0);
    assert_eq!(
        report.rejoins_succeeded + report.rejoins_too_late,
        report.rejoins_attempted
    );
    assert!(report.rejoins_succeeded > 0);
    assert!(report.snapshots_showing_disconnected > 0);
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(report.unexpected_closes, 0);
    assert_eq!(report.hash_mismatches, 0);
    running.shut_down(Duration::ZERO).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn bots_queue_through_the_front_door_accept_and_play_what_it_forms() {
    use arena_bot::queue::QueueScenario;
    use arena_server::door::{self, DoorConfig};
    let (running, mut scenario) = scenario(20, 5).await;
    let mut config = DoorConfig::new(
        running.app.key().clone(),
        format!("ws://{}/play", running.addr),
        "http://localhost:8000/".into(),
    );
    config.bind = SocketAddr::from(([127, 0, 0, 1], 0));
    let door = door::start(config).await.unwrap();
    scenario.queue = Some(QueueScenario {
        door: door.addr.to_string(),
        mode: "2v2".into(),
        bots: 4,
    });
    let report = run(scenario).await.unwrap();
    println!("{report}");
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(report.unexpected_closes, 0);
    assert_eq!((report.players, report.team_size), (4, 2));
    // All four were put in one match together: each started it.
    assert!(report.rounds_started >= 4, "{}", report.rounds_started);
    assert!(report.hash_checks > 0);
    assert_eq!(report.hash_mismatches, 0);
    door.stop().await.unwrap();
    running.shut_down(Duration::ZERO).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn bots_play_consistently_through_high_jitter_and_team_combat() {
    let (running, mut scenario) = scenario(180, 50).await;
    scenario.matches = 2;
    scenario.team_size = 3;
    scenario.duration = Duration::from_secs(2);
    let report = run(scenario).await.unwrap();
    println!("{report}");
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(report.unexpected_closes, 0);
    assert_eq!(report.wrong_results, 0);
    assert!(report.hash_checks > 50, "{}", report.hash_checks);
    assert_eq!(report.hash_mismatches, 0);
    assert!(report.rtt.p50_ms >= 180.0, "{}", report.rtt);
    let server = report.server_metrics.as_ref().unwrap();
    assert_eq!(server.frames_dropped_total, 0);
    assert_eq!(server.inputs_overflow_total, 0);
    running.shut_down(Duration::ZERO).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn bots_survive_tcp_packet_loss_and_relay_clumps_with_pacing() {
    let (running, mut scenario) = scenario(80, 20).await;
    scenario.matches = 2;
    scenario.team_size = 2;
    scenario.duration = Duration::from_secs(2);
    scenario.link = scenario
        .link
        .with_loss(0.05)
        .with_clump(Duration::from_millis(45));
    scenario.paced = true;
    let report = run(scenario).await.unwrap();
    println!("{report}");
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(report.unexpected_closes, 0);
    assert_eq!(report.wrong_results, 0);
    assert!(report.hash_checks > 40, "{}", report.hash_checks);
    assert_eq!(report.hash_mismatches, 0);
    assert!(report.loss_pct > 4.9 && report.clump_ms >= 45.0);
    assert!(report.paced);
    let server = report.server_metrics.as_ref().unwrap();
    assert!(server.ticks_total > 0);
    assert_eq!(server.frames_dropped_total, 0);
    assert_eq!(server.inputs_overflow_total, 0);
    running.shut_down(Duration::ZERO).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn bots_recover_across_aggressive_reconnect_jitter_and_loss_matrix() {
    let (running, mut scenario) = scenario(100, 30).await;
    scenario.matches = 2;
    scenario.team_size = 2;
    scenario.duration = Duration::from_millis(2500);
    scenario.link = scenario.link.with_loss(0.03);
    scenario.reconnect_every = Some(Duration::from_millis(550));
    scenario.away = Duration::from_millis(180);
    let report = run(scenario).await.unwrap();
    println!("{report}");
    assert!(report.rejoins_attempted > 0);
    assert_eq!(
        report.rejoins_succeeded + report.rejoins_too_late,
        report.rejoins_attempted
    );
    assert!(report.rejoins_succeeded > 0);
    assert!(report.snapshots_showing_disconnected > 0);
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(report.unexpected_closes, 0);
    assert_eq!(report.wrong_results, 0);
    assert_eq!(report.hash_mismatches, 0);
    running.shut_down(Duration::ZERO).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn bots_emit_structured_json_combat_telemetry_and_time_limit_draw_rate() {
    let mut config = Config::new(TicketKey::random());
    config.bind = SocketAddr::from(([127, 0, 0, 1], 0));
    config.dev = true;
    // 1-second rounds guarantee deterministic time-limit draws so outcome,
    // duration, and server tick-mean telemetry are all populated.
    config.round_ticks = 30;
    let running = start(config).await.unwrap();
    let scenario = Scenario {
        server: running.addr.to_string(),
        matches: 2,
        team_size: 1,
        duration: Duration::from_millis(2200),
        link: Link::round_trip(Duration::from_millis(20), Duration::ZERO),
        paced: true,
        interp_delay_ms: arena_bot::fairness::DEFAULT_INTERP_DELAY_MS,
        reconnect_every: None,
        away: Duration::from_millis(200),
        seed: 13,
        server_pid: None,
        queue: None,
    };
    let report = run(scenario).await.unwrap();
    println!("{report}");
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(report.unexpected_closes, 0);
    assert_eq!(report.wrong_results, 0);
    assert_eq!(report.hash_mismatches, 0);
    assert!(report.outcomes.matches_finished >= 2);
    assert_eq!(
        report.outcomes.time_limit_draws,
        report.outcomes.matches_finished
    );
    assert!((report.outcomes.draw_rate - 1.0).abs() < 1e-12);
    assert_eq!(
        report.round_duration.count as u64,
        report.outcomes.matches_finished
    );
    assert_eq!(report.round_duration.p50_ticks, 30);

    let server = report.server_metrics.as_ref().unwrap();
    assert!(server.ticks_total > 0);
    assert!(server.tick_mean_microseconds.is_finite());
    assert!(server.matches_finished_time_limit_draw >= 2);

    let json = serde_json::to_string_pretty(&report).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        parsed["outcomes"]["matches_finished"],
        report.outcomes.matches_finished
    );
    assert_eq!(parsed["round_duration"]["p50_ticks"], 30);
    assert!(parsed["combat"]["los_blocks"].is_u64());
    assert!(parsed["server_metrics"]["tick_mean_microseconds"].is_f64());
    assert!(parsed["targeting_fairness"]["casts_resolved"].is_u64());
    assert!(parsed["bandwidth"]["full_30hz_kbit_per_s_per_player"].is_f64());
    assert!(report.bandwidth.snapshots_30hz > 0);
    assert!(
        report.bandwidth.delta_30hz_kbit_per_s_per_player
            <= report.bandwidth.full_30hz_kbit_per_s_per_player
    );

    running.shut_down(Duration::ZERO).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn bots_measure_targeting_fairness_bandwidth_deltas_and_server_profile() {
    let (running, mut scenario) = scenario(100, 20).await;
    scenario.matches = 2;
    scenario.team_size = 2;
    // Teams spawn ~94 m apart in Colosseum (~5.7 s approach at 14.4 m/s combined);
    // 9 s ensures active ranged + close-quarters casting and fairness samples.
    scenario.duration = Duration::from_secs(9);
    scenario.interp_delay_ms = 100;
    let report = run(scenario).await.unwrap();
    println!("{report}");
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(report.unexpected_closes, 0);
    assert_eq!(report.hash_mismatches, 0);
    assert_eq!(report.interp_delay_ms, 100);
    // Active 2v2 combat (~3–4 s after approach) must resolve targeted casts and populate fairness metrics.
    assert!(
        report.targeting_fairness.casts_resolved > 0,
        "expected targeted casts to resolve: {:?}",
        report.targeting_fairness
    );
    assert!(report.targeting_fairness.casts_client_in_range > 0);
    // Bandwidth evaluator must measure live 30 Hz full vs delta and decimated rates.
    assert!(report.bandwidth.snapshots_30hz > 50);
    assert!(report.bandwidth.deltas_30hz > 0);
    assert!(
        report.bandwidth.delta_30hz_savings_pct > 15.0,
        "expected meaningful delta savings: {:?}",
        report.bandwidth
    );
    assert!(
        report.bandwidth.full_15hz_kbit_per_s_per_player
            < report.bandwidth.full_30hz_kbit_per_s_per_player
    );
    // On Linux `--server-pid` populates the full CPU / RSS / context-switch breakdown.
    #[cfg(target_os = "linux")]
    {
        let profile = report
            .server_profile
            .as_ref()
            .expect("linux server profile");
        assert!(profile.cpu_total_cores >= 0.0);
        assert!(profile.peak_rss_mib > 0.0);
        assert!(profile.threads >= 1);
    }
    running.shut_down(Duration::ZERO).await.unwrap();
}
