//! `arena-server [--bind ADDR] [--dev] [--debug-json] [--drain-seconds N]
//!  [--allow-origin URL]... [--any-origin] [--trusted-proxy CIDR]...
//!  [--connections-per-address N]`
//!
//! The signing key comes from `ARENA_TICKET_KEY` (hex, at least 32 bytes),
//! shared with whatever issues tickets (the front door, `arena-door`). With
//! `--dev` and no key, a random key is generated and `POST /dev/match`
//! issues tickets itself.
//!
//! Browsers may connect only from the pages given with `--allow-origin`
//! (the game page's origin, such as `https://arena.example.com`). Without
//! `--dev` one of `--allow-origin` or `--any-origin` is required. Behind a
//! reverse proxy, `--trusted-proxy` names the proxy's addresses so the
//! per-address connection limit counts players, not the proxy.
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

use arena_server::ingress::{Cidr, Origins};
use arena_server::ticket::TicketKey;
use arena_server::{Config, start};
use arena_sim::TICKS_PER_SECOND;
use arena_sim::arena::MAX_ROUND_TICKS;

const MAX_ROUND_SECONDS: u64 = MAX_ROUND_TICKS / TICKS_PER_SECOND;
const USAGE: &str = "usage: arena-server [--bind ADDR] [--dev] [--debug-json] [--drain-seconds N] \
[--round-seconds N] [--spectator-delay-seconds N] [--max-spectators N] [--allow-origin URL]... \
[--any-origin] [--trusted-proxy CIDR]... [--connections-per-address N]";
/// How often a drain reports the matches it is waiting for.
const DRAIN_REPORT: Duration = Duration::from_secs(10);

#[tokio::main]
async fn main() -> ExitCode {
    arena_server::logging::init();
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            tracing::error!(reason = %message, "arena-server failed");
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

/// Which pages may connect. Production must choose: a missing choice would
/// silently let any website use the server through its visitors.
fn origins_for(allowed: Vec<String>, any: bool, dev: bool) -> Result<Origins, String> {
    match (allowed.is_empty(), any) {
        (false, true) => Err("--allow-origin and --any-origin exclude each other".into()),
        (false, false) => Ok(Origins::Only(allowed)),
        (true, true) => Ok(Origins::Any),
        (true, false) if dev => Ok(Origins::Any),
        (true, false) => Err(
            "--allow-origin is required without --dev: give the game page's origin, such as \
             https://arena.example.com (repeat for more), or --any-origin to allow every page"
                .into(),
        ),
    }
}

/// Round duration in ticks (`1..=MAX_ROUND_SECONDS`, defaulting to 20 minutes).
fn round_ticks_for(seconds: Option<u64>) -> Result<u64, String> {
    match seconds {
        None => Ok(MAX_ROUND_TICKS),
        Some(secs) if (1..=MAX_ROUND_SECONDS).contains(&secs) => Ok(secs * TICKS_PER_SECOND),
        Some(_) => Err(format!(
            "--round-seconds must be between 1 and {MAX_ROUND_SECONDS}"
        )),
    }
}

async fn run() -> Result<(), String> {
    let mut bind: SocketAddr = SocketAddr::from(([127, 0, 0, 1], 8080));
    let mut dev = false;
    let mut debug_json = false;
    let mut drain_seconds = None;
    let mut round_seconds = None;
    let mut spectator_delay_seconds: Option<u64> = None;
    let mut max_spectators: Option<usize> = None;
    let mut allowed = Vec::new();
    let mut any_origin = false;
    let mut trusted_proxies = Vec::new();
    let mut connections_per_address = None;
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
            "--round-seconds" => {
                let value = args.next().ok_or(USAGE)?;
                round_seconds = Some(
                    value
                        .parse()
                        .map_err(|_| format!("invalid --round-seconds {value}"))?,
                );
            }
            "--spectator-delay-seconds" => {
                let value = args.next().ok_or(USAGE)?;
                let parsed: u64 = value
                    .parse()
                    .map_err(|_| format!("invalid --spectator-delay-seconds {value}"))?;
                if parsed > 60 {
                    return Err("--spectator-delay-seconds must be between 0 and 60".into());
                }
                spectator_delay_seconds = Some(parsed);
            }
            "--max-spectators" => {
                let value = args.next().ok_or(USAGE)?;
                let parsed: usize = value
                    .parse()
                    .map_err(|_| format!("invalid --max-spectators {value}"))?;
                if parsed > 64 {
                    return Err("--max-spectators must be between 0 and 64".into());
                }
                max_spectators = Some(parsed);
            }
            "--allow-origin" => {
                let value = args.next().ok_or(USAGE)?;
                allowed.push(Origins::parse_origin(&value).ok_or(format!(
                    "invalid --allow-origin {value}: expected an origin such as https://arena.example.com"
                ))?);
            }
            "--any-origin" => any_origin = true,
            "--trusted-proxy" => {
                let value = args.next().ok_or(USAGE)?;
                trusted_proxies
                    .push(Cidr::parse(&value).ok_or(format!("invalid --trusted-proxy {value}"))?);
            }
            "--connections-per-address" => {
                let value = args.next().ok_or(USAGE)?;
                connections_per_address = Some(
                    value
                        .parse()
                        .map_err(|_| format!("invalid --connections-per-address {value}"))?,
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
    let round_ticks = round_ticks_for(round_seconds)?;
    let origins = origins_for(allowed, any_origin, dev)?;
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
    config.origins = origins;
    config.trusted_proxies = trusted_proxies;
    config.round_ticks = round_ticks;
    if let Some(delay) = spectator_delay_seconds {
        config.limits.spectator_delay = delay * TICKS_PER_SECOND;
    }
    if let Some(cap) = max_spectators {
        config.limits.max_spectators = cap;
    }
    if let Some(limit) = connections_per_address {
        config.connections_per_address = limit;
    } else if dev {
        // Local bots open thousands of connections from one address.
        config.connections_per_address = 0;
    }
    let running = start(config).await.map_err(|error| error.to_string())?;
    tracing::info!(addr = %running.addr, "arena-server listening on ws://{}/play", running.addr);
    if dev {
        tracing::info!(
            "development mode; create a match with POST http://{}/dev/match?team_size=1",
            running.addr
        );
    }
    stop_signal().await;
    running.begin_drain();
    if running.active_matches() > 0 && !drain.is_zero() {
        tracing::info!(
            matches = running.active_matches(),
            drain_seconds = drain.as_secs(),
            "draining running matches; no new matches. Signal again to stop now."
        );
        let deadline = tokio::time::Instant::now() + drain;
        loop {
            let wait =
                DRAIN_REPORT.min(deadline.saturating_duration_since(tokio::time::Instant::now()));
            tokio::select! {
                finished = running.drained(wait) => {
                    if finished {
                        tracing::info!("drain finished: every match has ended");
                        break;
                    }
                    if tokio::time::Instant::now() >= deadline {
                        tracing::warn!(
                            matches = running.active_matches(),
                            reason = "drain_timeout",
                            "drain time is up; closing matches"
                        );
                        break;
                    }
                    tracing::info!(matches = running.active_matches(), "still draining");
                }
                _ = stop_signal() => {
                    tracing::warn!(
                        matches = running.active_matches(),
                        reason = "second_signal",
                        "stopping now; closing matches"
                    );
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

    #[test]
    fn production_must_choose_which_pages_may_connect() {
        let page = || vec!["https://arena.example.com".to_owned()];
        assert_eq!(origins_for(Vec::new(), false, true), Ok(Origins::Any));
        assert_eq!(origins_for(Vec::new(), true, false), Ok(Origins::Any));
        assert_eq!(origins_for(page(), false, false), Ok(Origins::Only(page())));
        assert!(origins_for(page(), true, false).is_err());
        let refused = origins_for(Vec::new(), false, false).unwrap_err();
        assert!(refused.contains("--allow-origin"), "{refused}");
    }

    #[test]
    fn round_seconds_defaults_to_twenty_minutes_and_rejects_out_of_range() {
        assert_eq!(round_ticks_for(None), Ok(MAX_ROUND_TICKS));
        assert_eq!(round_ticks_for(Some(1)), Ok(TICKS_PER_SECOND));
        assert_eq!(
            round_ticks_for(Some(MAX_ROUND_SECONDS)),
            Ok(MAX_ROUND_TICKS)
        );
        assert!(round_ticks_for(Some(0)).is_err());
        assert!(round_ticks_for(Some(MAX_ROUND_SECONDS + 1)).is_err());
    }
}
