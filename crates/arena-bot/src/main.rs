//! `arena-bot [--server HOST:PORT] [--matches N] [--team-size 1|2|3]
//!            [--seconds S] [--rtt MS] [--jitter MS] [--loss-pct PCT] [--clump-ms MS]
//!            [--unpaced] [--reconnect-every S] [--away-ms MS]
//!            [--seed N] [--server-pid PID] [--json]
//!            [--queue 1v1|2v2|3v3 --door HOST:PORT [--bots N]]`
//!
//! Plays bot matches against a server started with `--dev` and prints a
//! latency, consistency and recovery report. With `--queue`, `--bots` bots
//! instead queue through the front door at `--door` and play the matches it
//! forms, alongside any people in that queue. `--server` then only serves
//! the final metrics. Exits non-zero when a bot saw
//! an error, an unexpected close or a state hash mismatch.

use std::process::ExitCode;
use std::time::Duration;

use arena_bot::link::Link;
use arena_bot::queue::QueueScenario;
use arena_bot::{Scenario, run};

const USAGE: &str = "usage: arena-bot [--server HOST:PORT] [--matches N] [--team-size 1|2|3] [--seconds S] [--rtt MS] [--jitter MS] [--loss-pct PCT] [--clump-ms MS] [--unpaced] [--reconnect-every S] [--away-ms MS] [--seed N] [--server-pid PID] [--json] [--queue MODE --door HOST:PORT [--bots N]]";

#[tokio::main]
async fn main() -> ExitCode {
    match parse() {
        Err(message) => {
            eprintln!("arena-bot: {message}");
            ExitCode::FAILURE
        }
        Ok((None, _)) => ExitCode::SUCCESS,
        Ok((Some(scenario), json)) => match run(scenario).await {
            Err(message) => {
                eprintln!("arena-bot: {message}");
                ExitCode::FAILURE
            }
            Ok(report) => {
                if json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&report).expect("reports serialize")
                    );
                } else {
                    print!("{report}");
                }
                let healthy = report.errors.is_empty()
                    && report.unexpected_closes == 0
                    && report.hash_mismatches == 0
                    && report.wrong_results == 0
                    && report.rejoins_succeeded + report.rejoins_too_late
                        == report.rejoins_attempted;
                if healthy {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::FAILURE
                }
            }
        },
    }
}

fn parse() -> Result<(Option<Scenario>, bool), String> {
    let mut scenario = Scenario {
        server: "127.0.0.1:8080".into(),
        matches: 1,
        team_size: 1,
        duration: Duration::from_secs(10),
        link: Link::default(),
        paced: true,
        reconnect_every: None,
        away: Duration::from_secs(1),
        seed: 1,
        server_pid: None,
        queue: None,
    };
    let (mut rtt, mut jitter, mut loss_pct, mut clump_ms, mut json) =
        (0u64, 0u64, 0.0f64, 0u64, false);
    let (mut queue, mut door, mut bots) = (None, None, 1usize);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = |name: &str| -> Result<String, String> {
            args.next()
                .ok_or_else(|| format!("{name} needs a value\n{USAGE}"))
        };
        let number = |name: &str, text: String| -> Result<u64, String> {
            text.parse().map_err(|_| format!("invalid {name} {text}"))
        };
        match arg.as_str() {
            "--server" => scenario.server = value("--server")?,
            "--matches" => scenario.matches = number("--matches", value("--matches")?)? as usize,
            "--team-size" => {
                scenario.team_size = number("--team-size", value("--team-size")?)? as usize;
            }
            "--seconds" => {
                scenario.duration = Duration::from_secs(number("--seconds", value("--seconds")?)?);
            }
            "--rtt" => rtt = number("--rtt", value("--rtt")?)?,
            "--jitter" => jitter = number("--jitter", value("--jitter")?)?,
            "--loss-pct" => {
                let raw = value("--loss-pct")?;
                let parsed: f64 = raw
                    .parse()
                    .map_err(|_| format!("invalid --loss-pct {raw}"))?;
                if !parsed.is_finite() || !(0.0..=50.0).contains(&parsed) {
                    return Err(format!(
                        "--loss-pct must be between 0 and 50, got {raw}\n{USAGE}"
                    ));
                }
                loss_pct = parsed;
            }
            "--clump-ms" => {
                let parsed = number("--clump-ms", value("--clump-ms")?)?;
                if parsed > 10_000 {
                    return Err(format!(
                        "--clump-ms must be at most 10000, got {parsed}\n{USAGE}"
                    ));
                }
                clump_ms = parsed;
            }
            "--unpaced" => scenario.paced = false,
            "--away-ms" => {
                scenario.away = Duration::from_millis(number("--away-ms", value("--away-ms")?)?);
            }
            "--reconnect-every" => {
                scenario.reconnect_every = Some(Duration::from_secs(number(
                    "--reconnect-every",
                    value("--reconnect-every")?,
                )?));
            }
            "--seed" => scenario.seed = number("--seed", value("--seed")?)?,
            "--server-pid" => {
                scenario.server_pid = Some(number("--server-pid", value("--server-pid")?)? as u32);
            }
            "--json" => json = true,
            "--queue" => queue = Some(value("--queue")?),
            "--door" => door = Some(value("--door")?),
            "--bots" => bots = number("--bots", value("--bots")?)? as usize,
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok((None, false));
            }
            other => return Err(format!("unknown argument {other}\n{USAGE}")),
        }
    }
    scenario.link = Link::round_trip(Duration::from_millis(rtt), Duration::from_millis(jitter))
        .with_loss(loss_pct / 100.0)
        .with_clump(Duration::from_millis(clump_ms));
    scenario.queue = match (queue, door) {
        (Some(mode), Some(door)) => Some(QueueScenario { door, mode, bots }),
        (None, None) => None,
        _ => return Err(format!("--queue and --door go together\n{USAGE}")),
    };
    Ok((Some(scenario), json))
}
