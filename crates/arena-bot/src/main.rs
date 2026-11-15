//! `arena-bot [--server HOST:PORT] [--matches N] [--team-size 1|2|3]
//!            [--seconds S] [--rtt MS] [--jitter MS] [--reconnect-every S]
//!            [--seed N] [--server-pid PID] [--json]`
//!
//! Plays bot matches against a server started with `--dev` and prints a
//! latency, consistency and recovery report. Exits non-zero when a bot saw
//! an error, an unexpected close or a state hash mismatch.

use std::process::ExitCode;
use std::time::Duration;

use arena_bot::link::Link;
use arena_bot::{Scenario, run};

const USAGE: &str = "usage: arena-bot [--server HOST:PORT] [--matches N] [--team-size 1|2|3] [--seconds S] [--rtt MS] [--jitter MS] [--reconnect-every S] [--seed N] [--server-pid PID] [--json]";

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
        reconnect_every: None,
        seed: 1,
        server_pid: None,
    };
    let (mut rtt, mut jitter, mut json) = (0u64, 0u64, false);
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
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok((None, false));
            }
            other => return Err(format!("unknown argument {other}\n{USAGE}")),
        }
    }
    scenario.link = Link::round_trip(Duration::from_millis(rtt), Duration::from_millis(jitter));
    Ok((Some(scenario), json))
}
