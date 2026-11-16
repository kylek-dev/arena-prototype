//! `arena-server [--bind ADDR] [--dev] [--debug-json] [--drain-seconds N]`
//!
//! The signing key comes from `ARENA_TICKET_KEY` (hex, at least 32 bytes),
//! shared with whatever issues tickets. With `--dev` and no key, a random
//! key is generated and `POST /dev/match` issues tickets itself.
//!
//! On SIGTERM or Ctrl+C the server drains: readiness fails and no new match
//! starts, while running matches continue for up to `--drain-seconds`; then
//! the rest are closed with `ServerShutdown`. A second signal stops at once.
//! A match lives on one server process, so the drain should equal the time
//! the platform allows a terminating instance (Kubernetes'
//! `terminationGracePeriodSeconds`). Without `--dev` it must be given; with
//! `--dev` it defaults to zero, so Ctrl+C exits immediately.

use std::net::SocketAddr;
use std::process::ExitCode;
use std::time::Duration;

use arena_server::ticket::TicketKey;
use arena_server::{Config, start};

const USAGE: &str = "usage: arena-server [--bind ADDR] [--dev] [--debug-json] [--drain-seconds N]";
/// How often a drain reports the matches it is waiting for.
const DRAIN_REPORT: Duration = Duration::from_secs(10);

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("arena-server: {message}");
            ExitCode::FAILURE
        }
    }
}

/// How long to let running matches finish after a stop signal.
fn drain_for(seconds: Option<u64>, dev: bool) -> Result<Duration, String> {
    match (seconds, dev) {
        (Some(seconds), _) => Ok(Duration::from_secs(seconds)),
        (None, true) => Ok(Duration::ZERO),
        (None, false) => Err(
            "--drain-seconds is required without --dev: set it to the time your platform \
             allows a terminating instance (for example Kubernetes' \
             terminationGracePeriodSeconds), or 0 to close matches at once"
                .into(),
        ),
    }
}

async fn run() -> Result<(), String> {
    let mut bind: SocketAddr = SocketAddr::from(([127, 0, 0, 1], 8080));
    let mut dev = false;
    let mut debug_json = false;
    let mut drain_seconds = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--bind" => {
                let value = args.next().ok_or(USAGE)?;
                bind = value
                    .parse()
                    .map_err(|_| format!("invalid --bind {value}"))?;
            }
            "--dev" => dev = true,
            "--debug-json" => debug_json = true,
            "--drain-seconds" => {
                let value = args.next().ok_or(USAGE)?;
                drain_seconds = Some(
                    value
                        .parse()
                        .map_err(|_| format!("invalid --drain-seconds {value}"))?,
                );
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            other => return Err(format!("unknown argument {other}\n{USAGE}")),
        }
    }
    let drain = drain_for(drain_seconds, dev)?;
    let key = match std::env::var("ARENA_TICKET_KEY") {
        Ok(hex) => TicketKey::from_hex(&hex)
            .ok_or("ARENA_TICKET_KEY must be hex encoding at least 32 bytes")?,
        Err(_) if dev => TicketKey::random(),
        Err(_) => return Err("ARENA_TICKET_KEY is required without --dev".into()),
    };
    let mut config = Config::new(key);
    config.bind = bind;
    config.dev = dev;
    config.debug_json = debug_json;
    let running = start(config).await.map_err(|error| error.to_string())?;
    eprintln!("arena-server: listening on ws://{}/play", running.addr);
    if dev {
        eprintln!(
            "arena-server: development mode; create a match with POST http://{}/dev/match?team_size=1",
            running.addr
        );
    }
    stop_signal().await;
    running.begin_drain();
    if running.active_matches() > 0 && !drain.is_zero() {
        eprintln!(
            "arena-server: draining {} running match(es) for up to {}s; no new matches. Signal again to stop now.",
            running.active_matches(),
            drain.as_secs()
        );
        let deadline = tokio::time::Instant::now() + drain;
        loop {
            let wait =
                DRAIN_REPORT.min(deadline.saturating_duration_since(tokio::time::Instant::now()));
            tokio::select! {
                finished = running.drained(wait) => {
                    if finished {
                        eprintln!("arena-server: every match has ended");
                        break;
                    }
                    if tokio::time::Instant::now() >= deadline {
                        eprintln!("arena-server: drain time is up; closing {} match(es)", running.active_matches());
                        break;
                    }
                    eprintln!("arena-server: still draining: {} match(es) running", running.active_matches());
                }
                _ = stop_signal() => {
                    eprintln!("arena-server: stopping now; closing {} match(es)", running.active_matches());
                    break;
                }
            }
        }
    }
    running.stop().await.map_err(|error| error.to_string())
}

async fn stop_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut terminate = signal(SignalKind::terminate()).expect("SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = terminate.recv() => {}
        }
    }
    #[cfg(not(unix))]
    let _ = tokio::signal::ctrl_c().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_drain_is_explicit_in_production_and_zero_in_development() {
        assert_eq!(drain_for(None, true), Ok(Duration::ZERO));
        assert_eq!(drain_for(Some(300), true), Ok(Duration::from_secs(300)));
        assert_eq!(drain_for(Some(0), false), Ok(Duration::ZERO));
        assert_eq!(drain_for(Some(600), false), Ok(Duration::from_secs(600)));
        let refused = drain_for(None, false).unwrap_err();
        assert!(
            refused.contains("terminationGracePeriodSeconds"),
            "{refused}"
        );
    }
}
