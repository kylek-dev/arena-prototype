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
use crate::identity::store::ProviderId;
use crate::identity::{Identity, cookie_allowed};
use crate::ingress;
use crate::metrics::Metrics;

/// What the page learns about its player.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Me {
    /// The account id in hex, or `None` for a guest.
    pub account: Option<String>,
    pub admin: bool,
    /// Providers this door can log in with.
    pub providers: Vec<String>,
}

fn no_store(mut response: Response) -> Response {
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
pub(super) async fn account(door: &Door, headers: &HeaderMap, require_origin: bool) -> Option<u64> {
    let identity = door.identity.as_ref()?;
    if !cookie_allowed(&door.config.origins, headers, require_origin) {
        return None;
    }
    identity
        .session(headers, unix_now())
        .await
        .map(|session| session.account)
}

/// Whether the per-address limit on login routes refuses this request.
fn limited(door: &Door, peer: SocketAddr, headers: &HeaderMap) -> bool {
    let address = ingress::client_address(peer, headers, &door.config.trusted_proxies);
    door.auth_rate.take(address, Instant::now()).is_err()
}

pub(super) async fn me(State(door): State<Arc<Door>>, headers: HeaderMap) -> Response {
    let account = account(&door, &headers, false).await;
    let identity = door.identity.as_ref();
    no_store(
        Json(Me {
            account: account.map(|id| format!("{id:016x}")),
            admin: account
                .zip(identity)
                .is_some_and(|(id, identity)| identity.is_admin(id)),
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
    let Some(account) = account(&door, &headers, true).await else {
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
