//! `arena-door --world-server URL --page URL [--bind ADDR] [--dev]
//!  [--allow-origin URL]... [--any-origin] [--trusted-proxy CIDR]...
//!  [--ticket-minutes N] [--maps-dir PATH] [--news-dir PATH] [--world-status URL]`
//!
//! The front door (ADR 0045): creates matches for remote players and hands
//! out one join link per seat. `--world-server` is the world server's
//! `/play` URL and `--page` the game page, both as players reach them. The
//! signing key comes from `ARENA_TICKET_KEY`, the same key the world server
//! uses. The lobby is at `/lobby/`. `--world-status` is the world server's
//! base URL on the private network (`http://world:8080`): the door asks it
//! which matches ended, so their players leave the in-match state at once.
//!
//! Without `--dev` both URLs must be secure (`wss://`, `https://`), since
//! browsers refuse a plain `ws://` connection from an `https://` page and
//! the links carry credentials. Match creation is allowed from the game
//! page's origin unless `--allow-origin` or `--any-origin` says otherwise.
//! `--dev` allows plain URLs and any origin, and only binds to loopback.
//!
//! Accounts (ADR 0055) are on when `ARENA_SESSION_KEY` (hex, at least 32
//! bytes, not the ticket key) is set. Players then log in through WorkOS
//! AuthKit (`ARENA_WORKOS_CLIENT_ID`, `ARENA_WORKOS_API_KEY`), and accounts
//! live in the PostgreSQL database `ARENA_DATABASE_URL`, kept for accounts
//! alone. `ARENA_ADMIN_ACCOUNTS` lists admin account ids. Login cookies act
//! only for named pages, never with `--any-origin`. Admins' saved maps go
//! to `--maps-dir` (in memory without it); launcher news goes to
//! `--news-dir` (likewise).

use std::net::SocketAddr;
use std::process::ExitCode;
use std::time::Duration;

use arena_world::door::{DoorConfig, start};
use arena_world::identity::IdentityConfig;
use arena_world::ingress::{Cidr, Origins};
use arena_world::ticket::TicketKey;

const USAGE: &str = "usage: arena-door --world-server URL --page URL [--bind ADDR] [--dev] \
[--allow-origin URL]... [--any-origin] [--trusted-proxy CIDR]... [--ticket-minutes N] [--maps-dir PATH] [--news-dir PATH] [--world-status URL]";

fn main() -> ExitCode {
    // Telemetry starts before and stops after the runtime: the OTLP HTTP
    // exporter blocks, which an async context must not do.
    let telemetry = arena_world::logging::init("arena-door");
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
fn check_urls(world_server: &str, page: &str, dev: bool) -> Result<(), String> {
    let secure = world_server.starts_with("wss://");
    if !(secure || dev && world_server.starts_with("ws://")) {
        return Err(format!(
            "--world-server {world_server} must be a wss:// URL{}",
            if dev {
                " or ws:// URL"
            } else {
                " (ws:// only with --dev)"
            }
        ));
    }
    if !world_server.trim_end_matches('/').ends_with("/play") {
        return Err(format!(
            "--world-server {world_server} must be the world server's /play endpoint"
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
    let mut world_server = None;
    let mut page = None;
    let mut allowed = Vec::new();
    let mut any_origin = false;
    let mut trusted_proxies = Vec::new();
    let mut ticket_minutes = None;
    let mut maps = None;
    let mut news = None;
    let mut world_status = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or_else(|| USAGE.to_owned());
        match arg.as_str() {
            "--bind" => {
                let text = value()?;
                bind = text.parse().map_err(|_| format!("invalid --bind {text}"))?;
            }
            "--dev" => dev = true,
            // `--match-server` and `--match-status`: the names before the
            // world server was renamed (ADR 0057), still accepted.
            "--world-server" | "--match-server" => world_server = Some(value()?),
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
            "--maps-dir" => maps = Some(std::path::PathBuf::from(value()?)),
            "--news-dir" => news = Some(std::path::PathBuf::from(value()?)),
            "--world-status" | "--match-status" => {
                let text = value()?;
                if !(text.starts_with("http://") || text.starts_with("https://")) {
                    return Err(format!("--world-status {text} must be an http(s):// URL"));
                }
                world_status = Some(text);
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            other => return Err(format!("unknown argument {other}\n{USAGE}")),
        }
    }
    let world_server = world_server.ok_or(format!("--world-server is required\n{USAGE}"))?;
    let page = page.ok_or(format!("--page is required\n{USAGE}"))?;
    check_urls(&world_server, &page, dev)?;
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
        .map_err(|_| "ARENA_TICKET_KEY is required: the world server's signing key (hex)")?;
    let key = TicketKey::from_hex(&key)
        .ok_or("ARENA_TICKET_KEY must be hex encoding at least 32 bytes")?;
    let mut config = DoorConfig::new(key, world_server, page);
    config.bind = bind;
    config.origins = origins;
    config.trusted_proxies = trusted_proxies;
    if let Some(minutes) = ticket_minutes {
        config.ticket_ttl = Duration::from_secs(minutes * 60);
    }
    config.maps = maps;
    config.news = news;
    config.world_status = world_status;
    if dev {
        // A loopback door: local load tests (`arena-bot --hub`) open many
        // queue connections from one address.
        config.queue_per_address = config.queue_per_address.max(64);
    }
    config.identity = IdentityConfig::from_env()?;
    if std::env::var("ARENA_SESSION_KEY")
        .ok()
        .map(|k| k.trim().to_ascii_lowercase())
        == std::env::var("ARENA_TICKET_KEY")
            .ok()
            .map(|k| k.trim().to_ascii_lowercase())
        && config.identity.is_some()
    {
        return Err("ARENA_SESSION_KEY must differ from ARENA_TICKET_KEY".into());
    }
    match &config.identity {
        None => tracing::info!("accounts are off (no ARENA_SESSION_KEY): guests only"),
        Some(identity) => {
            if matches!(config.origins, Origins::Any) {
                tracing::warn!("accounts are on, but login cookies never act with --any-origin");
            }
            tracing::info!(
                workos_client = %identity.workos.client_id,
                "accounts are on (WorkOS login, PostgreSQL store)"
            );
        }
    }
    let running = start(config).await.map_err(|error| error.to_string())?;
    tracing::info!(
        addr = %running.addr,
        "arena-door listening on http://{}; lobby at /lobby/",
        running.addr
    );
    stop_signal().await;
    // Nothing to drain beyond requests already being answered: accounts
    // live in PostgreSQL.
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
