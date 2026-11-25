//! Login routes (ADR 0055, `docs/PROTOCOL.md` "Accounts"):
//!
//! - `GET /api/auth/me`: `{account, admin, providers}` for this browser.
//! - `GET /api/auth/<provider>/login`: starts a login (state and PKCE in a
//!   short-lived signed cookie) and redirects to the provider.
//! - `GET /api/auth/<provider>/callback`: checks the state, trades the code
//!   for the provider's permanent id, sets the session cookie and returns
//!   to the game page. Never a free-form redirect.
//! - `POST /api/auth/logout`, `POST /api/auth/logout_everywhere` and
//!   `POST /api/auth/delete`: need an allowed `Origin`.
//!
//! Account ids, codes, tokens and subjects are never logged.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Json;
use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::header::{CACHE_CONTROL, COOKIE, LOCATION, SET_COOKIE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use tokio::time::Instant;

use super::{Door, unix_now};
use crate::identity::cookie::{
    Login, OAUTH_COOKIE, OAUTH_SECONDS, SESSION_COOKIE, SESSION_SECONDS, Session, cookie,
    set_cookie,
};
use crate::identity::store::{Account, HandleError, ProviderId, RENAME_EVERY};
use crate::identity::{Identity, cookie_allowed};
use crate::ingress;
use crate::metrics::Metrics;
use arena_protocol::{Handle, MAX_HANDLE_NUMBER};

/// What the page learns about its player.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Me {
    /// The account id in hex, or `None` for a guest.
    pub account: Option<String>,
    pub admin: bool,
    /// Providers this door can log in with.
    pub providers: Vec<String>,
    /// The account's `Name#1234`, once picked.
    #[serde(default)]
    pub handle: Option<String>,
    /// When the handle may next change to another name (Unix seconds);
    /// `None` when it may now.
    #[serde(default)]
    pub rename_at: Option<u64>,
}

pub(super) fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn with_cookie(mut response: Response, value: String) -> Response {
    if let Ok(value) = HeaderValue::from_str(&value) {
        response.headers_mut().append(SET_COOKIE, value);
    }
    response
}

fn redirect(to: &str) -> Response {
    let mut response = StatusCode::SEE_OTHER.into_response();
    if let Ok(to) = HeaderValue::from_str(to) {
        response.headers_mut().insert(LOCATION, to);
    }
    no_store(response)
}

/// The logged-in account for a request, when cookies may act for it.
pub(super) async fn account(
    door: &Door,
    headers: &HeaderMap,
    require_origin: bool,
) -> Option<Account> {
    let identity = door.identity.as_ref()?;
    if !cookie_allowed(&door.config.origins, headers, require_origin) {
        return None;
    }
    identity
        .session(headers, unix_now())
        .await
        .map(|(_, account)| account)
}

/// A temporary `Guest#NNNN` handle for a player who has not picked one.
pub(super) fn guest_handle() -> Handle {
    let number = (crate::ticket::random_u64() % u64::from(MAX_HANDLE_NUMBER)) as u16 + 1;
    Handle::new("Guest", number).expect("Guest is a valid name")
}

/// Whether the per-address limit on login routes refuses this request.
pub(super) fn limited(door: &Door, peer: SocketAddr, headers: &HeaderMap) -> bool {
    let address = ingress::client_address(peer, headers, &door.config.trusted_proxies);
    door.auth_rate.take(address, Instant::now()).is_err()
}

pub(super) async fn me(State(door): State<Arc<Door>>, headers: HeaderMap) -> Response {
    let account = account(&door, &headers, false).await;
    let identity = door.identity.as_ref();
    let now = unix_now();
    no_store(
        Json(Me {
            account: account.map(|account| format!("{:016x}", account.id)),
            handle: account
                .and_then(|account| account.handle)
                .map(|h| h.to_string()),
            rename_at: account
                .filter(|account| account.handle.is_some())
                .and_then(|account| account.renamed)
                .map(|renamed| renamed.saturating_add(RENAME_EVERY))
                .filter(|at| *at > now),
            admin: account
                .zip(identity)
                .is_some_and(|(account, identity)| identity.is_admin(account.id)),
            providers: identity
                .map(|identity| {
                    identity
                        .providers
                        .iter()
                        .map(|p| p.id.id().to_owned())
                        .collect()
                })
                .unwrap_or_default(),
        })
        .into_response(),
    )
}

fn provider_of<'a>(door: &'a Door, name: &str) -> Option<(&'a Identity, ProviderId)> {
    let identity = door.identity.as_ref()?;
    let id = ProviderId::from_id(name)?;
    identity.provider(id)?;
    Some((identity, id))
}

pub(super) async fn login(
    State(door): State<Arc<Door>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(provider): Path<String>,
) -> Response {
    if limited(&door, peer, &headers) {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    let Some((identity, id)) = provider_of(&door, &provider) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let login = Login::start(id, unix_now());
    let to = identity
        .provider(id)
        .expect("checked above")
        .authorize(&login, &identity.redirect_uri(id));
    with_cookie(
        redirect(&to),
        set_cookie(OAUTH_COOKIE, &identity.key.login(&login), OAUTH_SECONDS),
    )
}

#[derive(Debug, Deserialize)]
pub(super) struct Callback {
    code: Option<String>,
    state: Option<String>,
}

pub(super) async fn callback(
    State(door): State<Arc<Door>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(provider): Path<String>,
    Query(callback): Query<Callback>,
) -> Response {
    if limited(&door, peer, &headers) {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    let Some((identity, id)) = provider_of(&door, &provider) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let clear = set_cookie(OAUTH_COOKIE, "", 0);
    let fail = |reason: &'static str| {
        Metrics::add(&door.metrics.login_failures, 1);
        tracing::info!(provider = id.id(), reason, "login failed");
        with_cookie(
            no_store(
                (
                    StatusCode::BAD_REQUEST,
                    "Login failed. Go back and try again.",
                )
                    .into_response(),
            ),
            clear.clone(),
        )
    };
    let started = headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|header| header.to_str().ok())
        .find_map(|header| cookie(header, OAUTH_COOKIE))
        .and_then(|value| identity.key.read_login(value, unix_now()));
    let Some(started) = started else {
        return fail("no_login_in_progress");
    };
    // The state proves this browser started this login (no login CSRF).
    if started.provider != id || callback.state.as_deref() != Some(started.state.as_str()) {
        return fail("state_mismatch");
    }
    let Some(code) = callback
        .code
        .filter(|code| !code.is_empty() && code.len() <= 2048)
    else {
        return fail("no_code");
    };
    let provider = identity.provider(id).expect("checked above");
    let subject = match provider
        .subject(
            &identity.client,
            &code,
            &started.verifier,
            &identity.redirect_uri(id),
        )
        .await
    {
        Ok(subject) => subject,
        Err(problem) => {
            tracing::warn!(provider = id.id(), reason = %problem, "login could not reach the provider");
            return fail("provider_error");
        }
    };
    let store = identity.store.clone();
    let now = unix_now();
    let account = tokio::task::spawn_blocking(move || {
        store.login(id, &subject, now, crate::ticket::random_u64)
    })
    .await;
    let Ok(Ok(account)) = account else {
        return fail("store_error");
    };
    Metrics::add(&door.metrics.logins, 1);
    tracing::info!(provider = id.id(), "player logged in");
    let session = Session {
        account: account.id,
        generation: account.generation,
        expires: now + SESSION_SECONDS,
    };
    let response = with_cookie(redirect(&door.config.page), clear);
    with_cookie(
        response,
        set_cookie(
            SESSION_COOKIE,
            &identity.key.session(session),
            SESSION_SECONDS,
        ),
    )
}

/// The end of a session, by kind.
#[derive(Clone, Copy)]
pub(super) enum Ending {
    /// Forget this browser's cookie.
    Logout,
    /// End every session of the account.
    Everywhere,
    /// Delete the account and everything stored about it.
    Delete,
}

pub(super) async fn end(
    door: Arc<Door>,
    peer: SocketAddr,
    headers: HeaderMap,
    ending: Ending,
) -> Response {
    if limited(&door, peer, &headers) {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    if door.identity.is_none() {
        return StatusCode::NOT_FOUND.into_response();
    }
    if !cookie_allowed(&door.config.origins, &headers, true) {
        return (StatusCode::FORBIDDEN, "origin not allowed").into_response();
    }
    let cleared = set_cookie(SESSION_COOKIE, "", 0);
    let Some(account) = account(&door, &headers, true)
        .await
        .map(|account| account.id)
    else {
        // Not logged in (or already ended): just make sure the cookie goes.
        return with_cookie(no_store(StatusCode::NO_CONTENT.into_response()), cleared);
    };
    let identity = door.identity.as_ref().expect("checked above");
    let store = identity.store.clone();
    let done = tokio::task::spawn_blocking(move || match ending {
        Ending::Logout => Ok(true),
        Ending::Everywhere => store.end_sessions(account),
        Ending::Delete => store.delete(account),
    })
    .await;
    match done {
        Ok(Ok(_)) => {
            if !matches!(ending, Ending::Logout) {
                // Open queue connections stop using the account now.
                let _ = door
                    .queue
                    .send(super::queue::Command::Revoke { account })
                    .await;
            }
            if matches!(ending, Ending::Delete) {
                Metrics::add(&door.metrics.deletions, 1);
                tracing::info!("account deleted");
            }
            with_cookie(no_store(StatusCode::NO_CONTENT.into_response()), cleared)
        }
        _ => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(super) async fn logout(
    State(door): State<Arc<Door>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    end(door, peer, headers, Ending::Logout).await
}

pub(super) async fn logout_everywhere(
    State(door): State<Arc<Door>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    end(door, peer, headers, Ending::Everywhere).await
}

pub(super) async fn delete(
    State(door): State<Arc<Door>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    end(door, peer, headers, Ending::Delete).await
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ChooseHandle {
    name: String,
}

/// `POST /api/auth/handle` `{"name": "Rook"}`: picks or changes the
/// account's handle. Answers `{"handle": "Rook#0042"}`, or `{"error": code}`
/// with 401 (not logged in), 403 (origin), 409 (`name_full`), 422
/// (`invalid_name`, `blocked_name`) or 429 (`too_soon`, with `rename_at`).
pub(super) async fn choose_handle(
    State(door): State<Arc<Door>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Result<Json<ChooseHandle>, axum::extract::rejection::JsonRejection>,
) -> Response {
    if limited(&door, peer, &headers) {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    let Some(identity) = door.identity.as_ref() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !cookie_allowed(&door.config.origins, &headers, true) {
        return (StatusCode::FORBIDDEN, "origin not allowed").into_response();
    }
    let Some(account) = account(&door, &headers, true).await else {
        return refusal(StatusCode::UNAUTHORIZED, "not_logged_in", None);
    };
    let Ok(Json(ChooseHandle { name })) = body else {
        return refusal(StatusCode::UNPROCESSABLE_ENTITY, "invalid_name", None);
    };
    if let Err(refused) = identity.blocked.check(&name) {
        return refusal(StatusCode::UNPROCESSABLE_ENTITY, refused.code(), None);
    }
    let store = identity.store.clone();
    let now = unix_now();
    let chosen = tokio::task::spawn_blocking(move || {
        store.set_handle(account.id, &name, now, crate::ticket::random_u64)
    })
    .await;
    match chosen {
        Ok(Ok(Ok(handle))) => {
            // Open queue connections show the new handle from now on.
            let _ = door
                .queue
                .send(super::queue::Command::Rename {
                    account: account.id,
                    handle,
                })
                .await;
            no_store(Json(serde_json::json!({ "handle": handle.to_string() })).into_response())
        }
        Ok(Ok(Err(HandleError::TooSoon(at)))) => {
            refusal(StatusCode::TOO_MANY_REQUESTS, "too_soon", Some(at))
        }
        Ok(Ok(Err(HandleError::Full))) => refusal(StatusCode::CONFLICT, "name_full", None),
        Ok(Ok(Err(HandleError::NoAccount))) => {
            refusal(StatusCode::UNAUTHORIZED, "not_logged_in", None)
        }
        _ => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(super) fn refusal(status: StatusCode, code: &str, rename_at: Option<u64>) -> Response {
    let mut body = serde_json::json!({ "error": code });
    if let Some(at) = rename_at {
        body["rename_at"] = at.into();
    }
    no_store((status, Json(body)).into_response())
}
