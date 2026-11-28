//! Friends (social-identity step 4):
//!
//! - `GET /api/friends`: `{friends: [{handle, relation, presence}], blocked}`.
//!   `relation` is `friend`, `incoming` or `outgoing`; `presence` (friends
//!   only) is `offline`, `online`, `searching`, `in_lobby` or `in_match`.
//! - `POST /api/friends/<action>` `{"handle": "Name#1234"}`, where `action`
//!   is `request`, `accept`, `decline`, `remove`, `block` or `unblock`.
//!   Answers `{"relation"}` for requests, 204 otherwise, or `{"error"}`.
//!
//! Players are found by handle only; a block looks exactly like a missing
//! player to the blocked one. Both players' open queue connections get
//! `friends_changed` so pages refresh.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Json;
use axum::extract::{ConnectInfo, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use tokio::time::Instant;

use super::auth::{account, limited, no_store, refusal};
use super::queue::{Command, Presence, ToClient};
use super::{Door, unix_now};
use crate::identity::cookie_allowed;
use crate::identity::friends::{FriendError, Relation};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Target {
    handle: String,
}

/// Where each account is, from the matchmaker.
async fn presence_of(door: &Door, accounts: Vec<u64>) -> Vec<Presence> {
    let count = accounts.len();
    let (reply, answer) = tokio::sync::oneshot::channel();
    if door
        .queue
        .send(Command::Presence { accounts, reply })
        .await
        .is_err()
    {
        return vec![Presence::Offline; count];
    }
    answer
        .await
        .unwrap_or_else(|_| vec![Presence::Offline; count])
}

pub(super) async fn list(State(door): State<Arc<Door>>, headers: HeaderMap) -> Response {
    let Some(identity) = door.identity.as_ref() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(me) = account(&door, &headers, false).await else {
        return refusal(StatusCode::UNAUTHORIZED, "not_logged_in", None);
    };
    // Each read asks the matchmaker task for presence: keep polling bounded.
    if door.friend_reads.take(me.id, Instant::now()).is_err() {
        return refusal(StatusCode::TOO_MANY_REQUESTS, "rate_limited", None);
    }
    let Ok(friends) = identity.store.friends_of(me.id).await else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let presence = presence_of(
        &door,
        friends.people.iter().map(|person| person.account).collect(),
    )
    .await;
    let people: Vec<serde_json::Value> = friends
        .people
        .iter()
        .zip(presence)
        // Someone without a handle yet cannot be shown or addressed.
        .filter_map(|(person, presence)| {
            let handle = person.handle.as_ref()?;
            let mut entry = serde_json::json!({
                "handle": handle,
                "relation": person.relation.id(),
            });
            if person.relation == Relation::Friend {
                entry["presence"] = presence.id().into();
            }
            Some(entry)
        })
        .collect();
    no_store(
        Json(serde_json::json!({ "friends": people, "blocked": friends.blocked })).into_response(),
    )
}

pub(super) async fn act(
    State(door): State<Arc<Door>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(action): Path<String>,
    body: Result<Json<Target>, axum::extract::rejection::JsonRejection>,
) -> Response {
    if limited(&door, peer, &headers) {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    let Some(identity) = door.identity.as_ref() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let action = match action.as_str() {
        "request" | "accept" | "decline" | "remove" | "block" | "unblock" => action,
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    if !cookie_allowed(&door.config.origins, &headers, true) {
        return (StatusCode::FORBIDDEN, "origin not allowed").into_response();
    }
    let Some(me) = account(&door, &headers, true).await else {
        return refusal(StatusCode::UNAUTHORIZED, "not_logged_in", None);
    };
    if door.friend_actions.take(me.id, Instant::now()).is_err() {
        return refusal(StatusCode::TOO_MANY_REQUESTS, "rate_limited", None);
    }
    let Ok(Json(Target { handle })) = body else {
        return refusal(StatusCode::UNPROCESSABLE_ENTITY, "bad_request", None);
    };
    if action == "request" {
        if me.handle.is_none() {
            return refusal(StatusCode::CONFLICT, "pick_a_name_first", None);
        }
        if door.friend_requests.take(me.id, Instant::now()).is_err() {
            return refusal(StatusCode::TOO_MANY_REQUESTS, "rate_limited", None);
        }
    }
    // Neither a block nor its end tells the other player anything.
    let tell_other = !matches!(action.as_str(), "block" | "unblock");
    // A block also stops their chat reaching this player's open tabs.
    let blocking = match action.as_str() {
        "block" => Some(true),
        "unblock" => Some(false),
        _ => None,
    };
    let store = &identity.store;
    let now = unix_now();
    type Done = (Option<u64>, Result<Option<Relation>, FriendError>);
    let done = async {
        let Some(other) = store.find_handle(&handle).await? else {
            // A missing handle must answer like a blocked one would, so no
            // action tells a blocked player apart from a missing one:
            // `request` says no such player, `block` quietly does nothing,
            // and the rest find nothing there.
            return Ok((
                None,
                match action.as_str() {
                    "request" => Err(FriendError::NoSuchPlayer),
                    "block" => Ok(None),
                    _ => Err(FriendError::NothingThere),
                },
            ));
        };
        let result = match action.as_str() {
            "request" => store.request_friend(me.id, other, now).await?.map(Some),
            "accept" => store.accept_friend(me.id, other).await?.map(|()| None),
            "decline" | "remove" => store.remove_friend(me.id, other).await?.map(|()| None),
            // Deleted meanwhile: as quiet as a missing handle.
            "block" => match store.block(me.id, other).await? {
                Err(FriendError::NoSuchPlayer) => Ok(None),
                done => done.map(|()| None),
            },
            _ => store.unblock(me.id, other).await?.map(|()| None),
        };
        Ok::<Done, crate::identity::store::StoreError>((Some(other), result))
    }
    .await;
    let Ok((other, result)) = done else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    match result {
        Ok(relation) => {
            if let (Some(blocked), Some(other)) = (blocking, other) {
                let _ = door
                    .queue
                    .send(Command::SetBlock {
                        account: me.id,
                        other,
                        blocked,
                    })
                    .await;
            }
            // Both pages refresh their lists (a block tells only the blocker).
            let mut tell = vec![me.id];
            if let Some(other) = other.filter(|_| tell_other) {
                tell.push(other);
            }
            for account in tell {
                let _ = door
                    .queue
                    .send(Command::Notify {
                        account,
                        message: ToClient::FriendsChanged {},
                    })
                    .await;
            }
            match relation {
                Some(relation) => {
                    no_store(Json(serde_json::json!({ "relation": relation.id() })).into_response())
                }
                None => no_store(StatusCode::NO_CONTENT.into_response()),
            }
        }
        Err(error) => {
            let status = match error {
                FriendError::NoSuchPlayer => StatusCode::NOT_FOUND,
                FriendError::Yourself => StatusCode::UNPROCESSABLE_ENTITY,
                FriendError::AlreadyFriends
                | FriendError::AlreadyAsked
                | FriendError::TooMany
                | FriendError::NothingThere => StatusCode::CONFLICT,
            };
            refusal(status, error.code(), None)
        }
    }
}
