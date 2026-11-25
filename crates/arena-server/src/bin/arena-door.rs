//! `arena-door --match-server URL --page URL [--bind ADDR] [--dev]
//!  [--allow-origin URL]... [--any-origin] [--trusted-proxy CIDR]...
//!  [--ticket-minutes N]`
//!
//! The front door (ADR 0045): creates matches for remote players and hands
//! out one join link per seat. `--match-server` is the match server's
//! `/play` URL and `--page` the game page, both as players reach them. The
//! signing key comes from `ARENA_TICKET_KEY`, the same key the match server
//! uses. The lobby is at `/lobby/`.
//!
//! Without `--dev` both URLs must be secure (`wss://`, `https://`), since
//! browsers refuse a plain `ws://` connection from an `https://` page and
//! the links carry credentials. Match creation is allowed from the game
//! page's origin unless `--allow-origin` or `--any-origin` says otherwise.
//! `--dev` allows plain URLs and any origin, and only binds to loopback.

use std::net::SocketAddr;
use std::process::ExitCode;
use std::time::Duration;

use arena_server::door::{DoorConfig, start};
use arena_server::ingress::{Cidr, Origins};
use arena_server::ticket::TicketKey;

const USAGE: &str = "usage: arena-door --match-server URL --page URL [--bind ADDR] [--dev] \
[--allow-origin URL]... [--any-origin] [--trusted-proxy CIDR]... [--ticket-minutes N]";

fn main() -> ExitCode {
    // Telemetry starts before and stops after the runtime: the OTLP HTTP
    // exporter blocks, which an async context must not do.
    let telemetry = arena_server::logging::init("arena-door");
    let code = match tokio::runtime::Runtime::new() {
        Ok(runtime) => {
            let result = runtime.block_on(run());
            drop(runtime);
            match result {
                Ok(()) => ExitCode::SUCCESS,
                Err(message) => {
                    tracing::error!(reason = %message, "arena-door failed");
                    ExitCode::FAILURE
                }
            }
        }
        Err(error) => {
            tracing::error!(reason = %error, "arena-door could not start its runtime");
            ExitCode::FAILURE
        }
    };
    telemetry.shutdown();
    code
}

/// The scheme and authority of a URL: `https://a.example:8443/x` gives
/// `https://a.example:8443`.
fn origin_of(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next()?;
    Origins::parse_origin(&format!("{scheme}://{authority}"))
}

/// Checks the two public URLs; production requires TLS for both.
fn check_urls(match_server: &str, page: &str, dev: bool) -> Result<(), String> {
    let secure = match_server.starts_with("wss://");
    if !(secure || dev && match_server.starts_with("ws://")) {
        return Err(format!(
            "--match-server {match_server} must be a wss:// URL{}",
            if dev {
                " or ws:// URL"
            } else {
                " (ws:// only with --dev)"
            }
        ));
    }
    if !match_server.trim_end_matches('/').ends_with("/play") {
        return Err(format!(
            "--match-server {match_server} must be the match server's /play endpoint"
        ));
    }
    let secure_page = page.starts_with("https://");
    if !(secure_page || dev && page.starts_with("http://")) || origin_of(page).is_none() {
        return Err(format!(
            "--page {page} must be an https:// URL{}",
            if dev {
                " or http:// URL"
            } else {
                " (http:// only with --dev)"
            }
        ));
    }
    Ok(())
}

async fn run() -> Result<(), String> {
    let mut bind: SocketAddr = SocketAddr::from(([127, 0, 0, 1], 8090));
    let mut dev = false;
    let mut match_server = None;
    let mut page = None;
    let mut allowed = Vec::new();
    let mut any_origin = false;
    let mut trusted_proxies = Vec::new();
    let mut ticket_minutes = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or_else(|| USAGE.to_owned());
        match arg.as_str() {
            "--bind" => {
                let text = value()?;
                bind = text.parse().map_err(|_| format!("invalid --bind {text}"))?;
            }
            "--dev" => dev = true,
            "--match-server" => match_server = Some(value()?),
            "--page" => page = Some(value()?),
            "--allow-origin" => {
                let text = value()?;
                allowed.push(
                    Origins::parse_origin(&text).ok_or(format!("invalid --allow-origin {text}"))?,
                );
            }
            "--any-origin" => any_origin = true,
            "--trusted-proxy" => {
                let text = value()?;
                trusted_proxies
                    .push(Cidr::parse(&text).ok_or(format!("invalid --trusted-proxy {text}"))?);
            }
            "--ticket-minutes" => {
                let text = value()?;
                ticket_minutes = Some(
                    text.parse::<u64>()
                        .ok()
                        .filter(|minutes| (1..=24 * 60).contains(minutes))
                        .ok_or(format!(
                            "invalid --ticket-minutes {text}: expected 1 to 1440"
                        ))?,
                );
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            other => return Err(format!("unknown argument {other}\n{USAGE}")),
        }
    }
    let match_server = match_server.ok_or(format!("--match-server is required\n{USAGE}"))?;
    let page = page.ok_or(format!("--page is required\n{USAGE}"))?;
    check_urls(&match_server, &page, dev)?;
    if dev && !bind.ip().is_loopback() {
        return Err("--dev requires a loopback --bind address".into());
    }
    if !allowed.is_empty() && any_origin {
        return Err("--allow-origin and --any-origin exclude each other".into());
    }
    let origins = if any_origin || dev && allowed.is_empty() {
        Origins::Any
    } else if allowed.is_empty() {
        Origins::Only(vec![origin_of(&page).expect("checked above")])
    } else {
        Origins::Only(allowed)
    };
    let key = std::env::var("ARENA_TICKET_KEY")
        .map_err(|_| "ARENA_TICKET_KEY is required: the match server's signing key (hex)")?;
    let key = TicketKey::from_hex(&key)
        .ok_or("ARENA_TICKET_KEY must be hex encoding at least 32 bytes")?;
    let mut config = DoorConfig::new(key, match_server, page);
    config.bind = bind;
    config.origins = origins;
    config.trusted_proxies = trusted_proxies;
    if let Some(minutes) = ticket_minutes {
        config.ticket_ttl = Duration::from_secs(minutes * 60);
    }
    let running = start(config).await.map_err(|error| error.to_string())?;
    tracing::info!(
        addr = %running.addr,
        "arena-door listening on http://{}; lobby at /lobby/",
        running.addr
    );
    stop_signal().await;
    // Stateless: nothing to drain beyond requests already being answered.
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
    fn origins_come_from_the_page_url() {
        assert_eq!(
            origin_of("https://Arena.example.com:8443/game/?x#y").as_deref(),
            Some("https://arena.example.com:8443")
        );
        assert_eq!(origin_of("arena.example.com"), None);
    }

    #[test]
    fn production_urls_must_be_secure() {
        let play = "wss://arena.example.com/play";
        let page = "https://arena.example.com/";
        assert_eq!(check_urls(play, page, false), Ok(()));
        assert!(check_urls("ws://arena.example.com/play", page, false).is_err());
        assert!(check_urls(play, "http://arena.example.com/", false).is_err());
        assert!(check_urls("wss://arena.example.com/", page, false).is_err());
        assert!(check_urls("https://arena.example.com/play", page, false).is_err());
        assert_eq!(
            check_urls("ws://127.0.0.1:8080/play", "http://localhost:8000/", true),
            Ok(())
        );
    }
}
