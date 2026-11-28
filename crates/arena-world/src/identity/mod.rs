//! Accounts (ADR 0055): players log in through WorkOS AuthKit (email and
//! password, an emailed code, Google or GitHub, so no social account is
//! needed); the door keeps the minimum about each account in its own
//! PostgreSQL database, holds the session in a signed cookie, and signs the
//! account id into match tickets. Guests keep playing without any of it.
//!
//! The world server never sees this module: it trusts the ticket alone.

pub mod cookie;
pub mod friends;
pub mod names;
pub mod store;
pub mod workos;

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use axum::http::HeaderMap;
use axum::http::header::{COOKIE, ORIGIN};

use crate::ingress::Origins;
use cookie::{SESSION_COOKIE, Session, SessionKey};
use store::{Account, Database, Store};
use workos::WorkOs;

/// How the door's accounts are set up.
#[derive(Clone, Debug)]
pub struct IdentityConfig {
    /// The accounts database (`ARENA_DATABASE_URL`).
    pub database: Database,
    pub key: SessionKey,
    pub workos: WorkOs,
    /// Accounts granted the admin role by deploy configuration.
    pub admins: HashSet<u64>,
    /// Words handles may not contain.
    pub blocked: names::Blocklist,
}

impl IdentityConfig {
    /// From `ARENA_SESSION_KEY` (hex, at least 32 bytes),
    /// `ARENA_DATABASE_URL`, `ARENA_WORKOS_CLIENT_ID`,
    /// `ARENA_WORKOS_API_KEY` and `ARENA_ADMIN_ACCOUNTS` (comma-separated
    /// account ids in hex). `Ok(None)` without a session key: the door is
    /// guest-only. With one, the rest are required.
    pub fn from_env() -> Result<Option<Self>, String> {
        let Some(key) = std::env::var("ARENA_SESSION_KEY")
            .ok()
            .filter(|key| !key.trim().is_empty())
        else {
            return Ok(None);
        };
        let key = SessionKey::from_hex(&key)
            .ok_or("ARENA_SESSION_KEY must be hex encoding at least 32 bytes")?;
        let admins = parse_admins(&std::env::var("ARENA_ADMIN_ACCOUNTS").unwrap_or_default())?;
        let url = std::env::var("ARENA_DATABASE_URL")
            .ok()
            .filter(|url| !url.trim().is_empty())
            .ok_or("accounts need ARENA_DATABASE_URL (postgres://...)")?;
        Ok(Some(Self {
            database: Database { url, schema: None },
            key,
            workos: WorkOs::from_env()?,
            admins,
            blocked: names::Blocklist::from_env()?,
        }))
    }
}

/// `ARENA_ADMIN_ACCOUNTS`: account ids in hex, separated by commas.
pub fn parse_admins(text: &str) -> Result<HashSet<u64>, String> {
    text.split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(|id| {
            u64::from_str_radix(id, 16)
                .ok()
                .filter(|id| *id != 0)
                .ok_or_else(|| format!("ARENA_ADMIN_ACCOUNTS: {id:?} is not an account id"))
        })
        .collect()
}

/// The door's accounts, once started.
#[derive(Debug)]
pub struct Identity {
    pub store: Arc<Store>,
    pub key: SessionKey,
    pub workos: WorkOs,
    pub admins: HashSet<u64>,
    pub blocked: names::Blocklist,
    pub client: reqwest::Client,
    /// The game page's origin: the callback is `{origin}/api/auth/callback`.
    pub origin: String,
}

impl Identity {
    pub async fn start(config: IdentityConfig, page: &str) -> Result<Self, String> {
        // HTTPS to WorkOS uses rustls with `ring`; installing it twice is
        // harmless.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let store = Store::connect(&config.database)
            .await
            .map_err(|error| format!("account store: {error}"))?;
        let origin = reqwest::Url::parse(page)
            .map_err(|_| format!("--page {page} is not a URL"))?
            .origin()
            .ascii_serialization();
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| format!("HTTPS client: {error}"))?;
        Ok(Self {
            store: Arc::new(store),
            key: config.key,
            workos: config.workos,
            admins: config.admins,
            blocked: config.blocked,
            client,
            origin,
        })
    }

    /// Where WorkOS sends the browser back (configured in its dashboard).
    pub fn redirect_uri(&self) -> String {
        format!("{}/api/auth/callback", self.origin)
    }

    pub fn is_admin(&self, account: u64) -> bool {
        self.admins.contains(&account)
    }

    /// The logged-in account behind a request's session cookie, if the
    /// cookie is signed, unexpired, and its account still exists at the same
    /// session generation.
    pub async fn session(&self, headers: &HeaderMap, now: u64) -> Option<(Session, Account)> {
        // HTTP/2 may split cookies over several headers.
        let value = headers
            .get_all(COOKIE)
            .iter()
            .filter_map(|header| header.to_str().ok())
            .find_map(|header| cookie::cookie(header, SESSION_COOKIE))?;
        let session = self.key.read_session(value, now)?;
        let account = self.store.get(session.account).await.ok()??;
        (account.generation == session.generation).then_some((session, account))
    }
}

/// Whether a request may act with its session cookie: the door must name
/// its allowed pages (never `--any-origin`), and the request must carry one
/// of them as its `Origin`. Reading `/api/auth/me` needs no `Origin`.
pub fn cookie_allowed(origins: &Origins, headers: &HeaderMap, require_origin: bool) -> bool {
    if matches!(origins, Origins::Any) {
        return false;
    }
    !require_origin || (headers.contains_key(ORIGIN) && origins.allows(headers))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn admins_come_from_a_list_of_account_ids() {
        assert_eq!(parse_admins("").unwrap(), HashSet::new());
        assert_eq!(
            parse_admins(" 1f, 00000000000000ab ").unwrap(),
            HashSet::from([0x1f, 0xab])
        );
        assert!(parse_admins("0").is_err());
        assert!(parse_admins("someone@example.com").is_err());
    }

    #[test]
    fn cookies_act_only_for_named_pages_with_an_origin() {
        let only = Origins::Only(vec!["https://arena.example.com".into()]);
        let mut headers = HeaderMap::new();
        assert!(
            cookie_allowed(&only, &headers, false),
            "reading needs no origin"
        );
        assert!(!cookie_allowed(&only, &headers, true), "acting does");
        headers.insert(ORIGIN, HeaderValue::from_static("https://evil.example"));
        assert!(!cookie_allowed(&only, &headers, true));
        headers.insert(
            ORIGIN,
            HeaderValue::from_static("https://arena.example.com"),
        );
        assert!(cookie_allowed(&only, &headers, true));
        assert!(
            !cookie_allowed(&Origins::Any, &headers, true),
            "never with --any-origin"
        );
        assert!(!cookie_allowed(&Origins::Any, &headers, false));
    }
}
