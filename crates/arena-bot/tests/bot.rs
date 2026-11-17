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
async fn bots_queue_through_the_front_door_and_play_what_it_forms() {
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
        mode: "solo-2v2".into(),
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
