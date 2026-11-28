//! Admin tools (social-identity step 5). The role comes from deploy
//! configuration (`ARENA_ADMIN_ACCOUNTS`) and is checked here, on the
//! server, for every request; hiding a button is never the gate. The first
//! tool is map storage for the in-game editor:
//!
//! - `GET /api/admin/maps`: `{maps: [{id, name}]}`.
//! - `GET /api/admin/maps/<id>`: the map file.
//! - `PUT /api/admin/maps/<id>`: validates a map file with `arena_map` and
//!   stores it; answers `{id, name}` or `{error: "invalid_map", problems}`.
//!
//! Maps live in the `--maps-dir` directory (in memory
//! without one). Matches still play the built-in colosseum.
//!
//! - `POST /api/admin/mute` `{"handle": "Name#1234", "minutes": 60}`: stops
//!   a logged-in player's chat for that long (0 unmutes; at most a week).
//! - `POST /api/admin/handles/clear` `{"handle": "Name#1234"}`: removes a
//!   player's handle (for a name that breaks the rules, for example one a
//!   newer blocked-word list covers); they pick again. 204, or 404
//!   `no_such_player`.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::Json;
use axum::body::Bytes;
use axum::extract::{ConnectInfo, Path, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use std::net::SocketAddr;

use super::Door;
use super::auth::{account, limited, no_store, refusal};
use crate::identity::cookie_allowed;
use crate::identity::store::Account;

/// Largest map file accepted (the editor's own limit).
pub(super) const MAX_MAP: usize = 256 * 1024;

/// Where admin-saved maps are kept.
#[derive(Debug)]
pub(super) enum MapStore {
    Directory(PathBuf),
    Memory(Mutex<BTreeMap<String, String>>),
}

impl MapStore {
    pub(super) fn at(directory: Option<&std::path::Path>) -> Self {
        match directory {
            Some(directory) => Self::Directory(directory.to_owned()),
            None => Self::Memory(Mutex::default()),
        }
    }

    fn list(&self) -> std::io::Result<Vec<(String, String)>> {
        let files: Vec<(String, String)> = match self {
            Self::Directory(directory) => {
                let Ok(entries) = std::fs::read_dir(directory) else {
                    return Ok(Vec::new());
                };
                let mut files = Vec::new();
                for entry in entries.flatten() {
                    // Only regular files: a directory, FIFO or link named
                    // `*.json` must not fail or hang the listing.
                    if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
                        continue;
                    }
                    let path = entry.path();
                    let Some(id) = path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .and_then(|name| name.strip_suffix(".json"))
                        .filter(|id| valid_id(id))
                    else {
                        continue;
                    };
                    // One unreadable file hides itself, not the list.
                    if let Ok(json) = std::fs::read_to_string(&path) {
                        files.push((id.to_owned(), json));
                    }
                }
                files
            }
            Self::Memory(maps) => maps
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .iter()
                .map(|(id, json)| (id.clone(), json.clone()))
                .collect(),
        };
        let mut listed: Vec<(String, String)> = files
            .into_iter()
            .filter_map(|(id, json)| arena_map::parse(&json).ok().map(|map| (id, map.name)))
            .collect();
        listed.sort();
        Ok(listed)
    }

    fn get(&self, id: &str) -> std::io::Result<Option<String>> {
        match self {
            Self::Directory(directory) => {
                match std::fs::read_to_string(directory.join(format!("{id}.json"))) {
                    Ok(json) => Ok(Some(json)),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                    Err(error) => Err(error),
                }
            }
            Self::Memory(maps) => Ok(maps
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .get(id)
                .cloned()),
        }
    }

    /// Writes atomically: a temporary file renamed into place.
    fn put(&self, id: &str, json: &str) -> std::io::Result<()> {
        match self {
            Self::Directory(directory) => {
                std::fs::create_dir_all(directory)?;
                // A temporary name per save, so two saves of one map never
                // write the same file; synced before it replaces the map.
                let temporary =
                    directory.join(format!(".{id}.{:016x}.tmp", crate::ticket::random_u64()));
                let written = (|| {
                    use std::io::Write;
                    let mut file = std::fs::File::create(&temporary)?;
                    file.write_all(json.as_bytes())?;
                    file.sync_all()?;
                    std::fs::rename(&temporary, directory.join(format!("{id}.json")))
                })();
                if written.is_err() {
                    let _ = std::fs::remove_file(&temporary);
                }
                written
            }
            Self::Memory(maps) => {
                maps.lock()
                    .unwrap_or_else(|poison| poison.into_inner())
                    .insert(id.to_owned(), json.to_owned());
                Ok(())
            }
        }
    }
}

/// Map ids as the editor names them: lowercase letters, digits and dashes.
pub(super) fn valid_id(id: &str) -> bool {
    (1..=41).contains(&id.len())
        && id.as_bytes()[0].is_ascii_alphanumeric()
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// The logged-in admin behind a request, or the refusal to send: 404 when
/// accounts are off, 403 for another page, 401 for a guest, 403
/// `not_admin` for everyone else. Writes also need an allowed `Origin`.
pub(super) async fn require_admin(
    door: &Door,
    headers: &HeaderMap,
    write: bool,
) -> Result<Account, Box<Response>> {
    let Some(identity) = door.identity.as_ref() else {
        return Err(Box::new(StatusCode::NOT_FOUND.into_response()));
    };
    if !cookie_allowed(&door.config.origins, headers, write) {
        return Err(Box::new(
            (StatusCode::FORBIDDEN, "origin not allowed").into_response(),
        ));
    }
    let Some(me) = account(door, headers, write).await else {
        return Err(Box::new(refusal(
            StatusCode::UNAUTHORIZED,
            "not_logged_in",
            None,
        )));
    };
    if !identity.is_admin(me.id) {
        return Err(Box::new(refusal(StatusCode::FORBIDDEN, "not_admin", None)));
    }
    Ok(me)
}

pub(super) async fn list_maps(
    State(door): State<Arc<Door>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    if limited(&door, peer, &headers) {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    if let Err(refused) = require_admin(&door, &headers, false).await {
        return *refused;
    }
    let maps = door.maps.clone();
    match tokio::task::spawn_blocking(move || maps.list()).await {
        Ok(Ok(listed)) => no_store(
            Json(serde_json::json!({
                "maps": listed
                    .into_iter()
                    .map(|(id, name)| serde_json::json!({ "id": id, "name": name }))
                    .collect::<Vec<_>>()
            }))
            .into_response(),
        ),
        _ => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(super) async fn get_map(
    State(door): State<Arc<Door>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(file): Path<String>,
) -> Response {
    if limited(&door, peer, &headers) {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    if let Err(refused) = require_admin(&door, &headers, false).await {
        return *refused;
    }
    let Some(id) = file
        .strip_suffix(".json")
        .filter(|id| valid_id(id))
        .map(str::to_owned)
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let maps = door.maps.clone();
    match tokio::task::spawn_blocking(move || maps.get(&id)).await {
        Ok(Ok(Some(json))) => {
            let mut response = no_store(json.into_response());
            response
                .headers_mut()
                .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
            response
        }
        Ok(Ok(None)) => StatusCode::NOT_FOUND.into_response(),
        _ => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(super) async fn put_map(
    State(door): State<Arc<Door>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(file): Path<String>,
    body: Result<Bytes, axum::extract::rejection::BytesRejection>,
) -> Response {
    if limited(&door, peer, &headers) {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    if let Err(refused) = require_admin(&door, &headers, true).await {
        return *refused;
    }
    let Some(id) = file
        .strip_suffix(".json")
        .filter(|id| valid_id(id))
        .map(str::to_owned)
    else {
        return refusal(StatusCode::UNPROCESSABLE_ENTITY, "invalid_id", None);
    };
    let body = match body {
        Ok(body) => body,
        Err(rejection) => return rejection.status().into_response(),
    };
    let Ok(text) = std::str::from_utf8(&body) else {
        return refusal(StatusCode::UNPROCESSABLE_ENTITY, "invalid_map", None);
    };
    let map = match arena_map::parse(text) {
        Ok(map) => map,
        Err(problems) => {
            return no_store(
                (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(serde_json::json!({ "error": "invalid_map", "problems": problems })),
                )
                    .into_response(),
            );
        }
    };
    // Stored as the canonical file the editor and build read.
    let json = map.to_json();
    let name = map.name.clone();
    let maps = door.maps.clone();
    let stored_id = id.clone();
    match tokio::task::spawn_blocking(move || maps.put(&stored_id, &json)).await {
        Ok(Ok(())) => {
            tracing::info!(map = %id, "admin saved a map");
            no_store(Json(serde_json::json!({ "id": id, "name": name })).into_response())
        }
        _ => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ClearHandle {
    handle: String,
}

pub(super) async fn clear_handle(
    State(door): State<Arc<Door>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Result<Json<ClearHandle>, axum::extract::rejection::JsonRejection>,
) -> Response {
    if limited(&door, peer, &headers) {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    if let Err(refused) = require_admin(&door, &headers, true).await {
        return *refused;
    }
    let Ok(Json(ClearHandle { handle })) = body else {
        return refusal(StatusCode::UNPROCESSABLE_ENTITY, "bad_request", None);
    };
    let store = door
        .identity
        .as_ref()
        .expect("admins need accounts")
        .store
        .clone();
    let cleared = async {
        let Some(id) = store.find_handle(&handle).await? else {
            return Ok(None);
        };
        Ok::<_, crate::identity::store::StoreError>(store.clear_handle(id).await?.then_some(id))
    }
    .await;
    match cleared {
        Ok(Some(account)) => {
            // Open searches stop showing the old name at once.
            let _ = door
                .queue
                .send(super::queue::Command::Rename {
                    account,
                    handle: None,
                })
                .await;
            tracing::info!("admin cleared a handle");
            no_store(StatusCode::NO_CONTENT.into_response())
        }
        Ok(None) => refusal(StatusCode::NOT_FOUND, "no_such_player", None),
        _ => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Mute {
    handle: String,
    minutes: u32,
}

/// Longest mute.
const MAX_MUTE_MINUTES: u32 = 7 * 24 * 60;

pub(super) async fn mute(
    State(door): State<Arc<Door>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Result<Json<Mute>, axum::extract::rejection::JsonRejection>,
) -> Response {
    if limited(&door, peer, &headers) {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    if let Err(refused) = require_admin(&door, &headers, true).await {
        return *refused;
    }
    let Ok(Json(Mute { handle, minutes })) = body else {
        return refusal(StatusCode::UNPROCESSABLE_ENTITY, "bad_request", None);
    };
    if minutes > MAX_MUTE_MINUTES {
        return refusal(StatusCode::UNPROCESSABLE_ENTITY, "too_long", None);
    }
    let store = door
        .identity
        .as_ref()
        .expect("admins need accounts")
        .store
        .clone();
    let Ok(found) = store.find_handle(&handle).await else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let Some(account) = found else {
        return refusal(StatusCode::NOT_FOUND, "no_such_player", None);
    };
    let mut muted = door
        .muted
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let now = tokio::time::Instant::now();
    muted.retain(|_, until| *until > now);
    if minutes == 0 {
        muted.remove(&account);
    } else {
        muted.insert(
            account,
            now + std::time::Duration::from_secs(u64::from(minutes) * 60),
        );
    }
    tracing::info!(minutes, "admin changed a mute");
    no_store(StatusCode::NO_CONTENT.into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_ids_are_plain_names() {
        for good in ["colosseum", "pit-2", "a"] {
            assert!(valid_id(good), "{good}");
        }
        for bad in [
            "",
            "-pit",
            "Pit",
            "../etc",
            "pit.json",
            "a/b",
            &"a".repeat(42),
        ] {
            assert!(!valid_id(bad), "{bad}");
        }
    }

    #[test]
    fn the_listing_skips_what_is_not_a_readable_map_file() {
        let directory =
            std::env::temp_dir().join(format!("arena-maps-{}", crate::ticket::random_u64()));
        let store = MapStore::Directory(directory.clone());
        store.put("pit", &arena_map::colosseum().to_json()).unwrap();
        std::fs::create_dir_all(directory.join("folder.json")).unwrap();
        std::fs::write(directory.join("broken.json"), [0xff, 0xfe]).unwrap();
        std::fs::write(directory.join("other.txt"), "x").unwrap();
        let listed: Vec<String> = store
            .list()
            .unwrap()
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        assert_eq!(listed, ["pit"]);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn maps_round_trip_through_a_directory() {
        let directory =
            std::env::temp_dir().join(format!("arena-maps-{}", crate::ticket::random_u64()));
        let store = MapStore::Directory(directory.clone());
        assert!(
            store.list().unwrap().is_empty(),
            "missing directory lists nothing"
        );
        let json = arena_map::colosseum().to_json();
        store.put("pit", &json).unwrap();
        assert_eq!(store.get("pit").unwrap().as_deref(), Some(json.as_str()));
        assert_eq!(store.get("nope").unwrap(), None);
        assert_eq!(
            store.list().unwrap(),
            [("pit".to_owned(), arena_map::colosseum().name)]
        );
        let _ = std::fs::remove_dir_all(directory);
    }
}
