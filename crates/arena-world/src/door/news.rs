//! News for the launcher page: short items admins post, newest first.
//!
//! - `GET /api/news`: `{news: [{id, title, body, posted, author}]}`, public.
//! - `POST /api/admin/news` `{title, body}`: an admin posts an item; 201
//!   with the item, or 422 `{error: "invalid_news", problem}`.
//! - `DELETE /api/admin/news/<id>`: an admin removes an item; 204 or 404.
//!
//! Items are plain text (the page never renders them as HTML) and live in
//! `news.json` in the `--news-dir` directory, written atomically (in memory
//! without one). The role is checked on the server for every change, as for
//! the other admin tools.

use std::net::SocketAddr;
use std::path::{Path as FilePath, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use axum::Json;
use axum::extract::{ConnectInfo, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use super::admin::require_admin;
use super::auth::{limited, no_store, refusal};
use super::{Door, unix_now};

/// Items kept: the oldest go once there are more.
pub(super) const MAX_ITEMS: usize = 50;
/// Characters (not bytes) in a title and a body.
pub(super) const MAX_TITLE: usize = 80;
pub(super) const MAX_BODY: usize = 2000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewsItem {
    /// 16 hexadecimal digits.
    pub id: String,
    pub title: String,
    pub body: String,
    /// Unix seconds.
    pub posted: u64,
    /// The posting admin's `Name#1234`, when they had one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
}

/// Why a post was refused (`problem` in the 422 answer).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Problem {
    EmptyTitle,
    LongTitle,
    EmptyBody,
    LongBody,
}

impl Problem {
    pub(super) fn code(self) -> &'static str {
        match self {
            Self::EmptyTitle => "empty_title",
            Self::LongTitle => "long_title",
            Self::EmptyBody => "empty_body",
            Self::LongBody => "long_body",
        }
    }
}

/// Plain text as stored: line endings as `\n` (a space in a title), other
/// control and invisible layout characters (bidi marks and overrides,
/// zero-width, line and paragraph separators) dropped, trimmed.
fn clean(text: &str, lines: bool) -> String {
    let text = text.replace("\r\n", "\n");
    let kept: String = text
        .chars()
        .map(|c| if c == '\n' && !lines { ' ' } else { c })
        .filter(|c| (lines && *c == '\n') || !c.is_control())
        .filter(|c| {
            !matches!(
                c,
                '\u{061C}'
                    | '\u{200B}'..='\u{200F}'
                    | '\u{2028}'
                    | '\u{2029}'
                    | '\u{202A}'..='\u{202E}'
                    | '\u{2066}'..='\u{2069}'
                    | '\u{FEFF}'
            )
        })
        .collect();
    kept.trim().to_owned()
}

/// A title and body as they would be stored, or why not.
pub(super) fn checked(title: &str, body: &str) -> Result<(String, String), Problem> {
    let title = clean(title, false);
    let body = clean(body, true);
    match (title.chars().count(), body.chars().count()) {
        (0, _) => Err(Problem::EmptyTitle),
        (n, _) if n > MAX_TITLE => Err(Problem::LongTitle),
        (_, 0) => Err(Problem::EmptyBody),
        (_, n) if n > MAX_BODY => Err(Problem::LongBody),
        _ => Ok((title, body)),
    }
}

/// The news, in memory and (with a directory) in `news.json`. Readers take
/// a shared snapshot and never wait for a write's disk sync; writes take
/// turns.
#[derive(Debug)]
pub(super) struct News {
    file: Option<PathBuf>,
    items: RwLock<Arc<Vec<NewsItem>>>,
    writing: Mutex<()>,
}

impl News {
    /// Reads `news.json` from `directory` (see [`load`]).
    pub(super) fn at(directory: Option<&FilePath>) -> Self {
        let file = directory.map(|directory| directory.join("news.json"));
        let items = file.as_deref().map(load).unwrap_or_default();
        Self {
            file,
            items: RwLock::new(Arc::new(items)),
            writing: Mutex::new(()),
        }
    }

    /// Newest first: a snapshot, cheap to take.
    pub(super) fn list(&self) -> Arc<Vec<NewsItem>> {
        self.items
            .read()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone()
    }

    fn replace(&self, next: Vec<NewsItem>) {
        *self
            .items
            .write()
            .unwrap_or_else(|poison| poison.into_inner()) = Arc::new(next);
    }

    /// Adds an item at the top and saves; the oldest beyond `MAX_ITEMS` go.
    pub(super) fn post(&self, item: NewsItem) -> std::io::Result<()> {
        let _turn = self
            .writing
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let mut next = (*self.list()).clone();
        next.insert(0, item);
        next.truncate(MAX_ITEMS);
        self.save(&next)?;
        self.replace(next);
        Ok(())
    }

    /// Removes an item and saves; `false` when there is none by that id.
    pub(super) fn delete(&self, id: &str) -> std::io::Result<bool> {
        let _turn = self
            .writing
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let current = self.list();
        let next: Vec<NewsItem> = current
            .iter()
            .filter(|item| item.id != id)
            .cloned()
            .collect();
        if next.len() == current.len() {
            return Ok(false);
        }
        self.save(&next)?;
        self.replace(next);
        Ok(true)
    }

    /// Writes atomically: a temporary file, synced, renamed into place, and
    /// the folder synced so the rename survives a crash.
    fn save(&self, items: &[NewsItem]) -> std::io::Result<()> {
        let Some(file) = &self.file else {
            return Ok(());
        };
        let directory = file.parent().unwrap_or(FilePath::new("."));
        std::fs::create_dir_all(directory)?;
        let temporary = directory.join(format!(".news.{:016x}.tmp", crate::ticket::random_u64()));
        let json = serde_json::to_vec_pretty(items).map_err(std::io::Error::other)?;
        let written = (|| {
            use std::io::Write;
            let mut out = std::fs::File::create(&temporary)?;
            out.write_all(&json)?;
            out.sync_all()?;
            std::fs::rename(&temporary, file)?;
            if let Ok(folder) = std::fs::File::open(directory) {
                let _ = folder.sync_all();
            }
            Ok(())
        })();
        if written.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        written
    }
}

/// Reads `news.json`, keeping only items that pass today's checks (a
/// hand-edited file is no exception), newest first, at most `MAX_ITEMS`. A
/// file that cannot be read as news is set aside, not overwritten by the
/// next post, and logged.
fn load(file: &FilePath) -> Vec<NewsItem> {
    let Ok(json) = std::fs::read_to_string(file) else {
        return Vec::new();
    };
    match serde_json::from_str::<Vec<NewsItem>>(&json) {
        Ok(items) => {
            let mut kept: Vec<NewsItem> = items
                .into_iter()
                .filter(|item| {
                    item.id.len() == 16
                        && item.id.bytes().all(|b| b.is_ascii_hexdigit())
                        && checked(&item.title, &item.body)
                            .is_ok_and(|(title, body)| title == item.title && body == item.body)
                })
                .collect();
            kept.sort_by_key(|item| std::cmp::Reverse(item.posted));
            kept.truncate(MAX_ITEMS);
            kept
        }
        Err(_) => {
            let aside = file.with_extension(format!("json.unreadable-{}", unix_now()));
            let _ = std::fs::rename(file, &aside);
            tracing::warn!(file = %aside.display(), "news file unreadable; set aside");
            Vec::new()
        }
    }
}

pub(super) async fn list(State(door): State<Arc<Door>>) -> Response {
    // Every visitor reads it: kept fresh, never stored by proxies.
    no_store(Json(serde_json::json!({ "news": *door.news.list() })).into_response())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Post {
    title: String,
    body: String,
}

pub(super) async fn post(
    State(door): State<Arc<Door>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    post: Result<Json<Post>, axum::extract::rejection::JsonRejection>,
) -> Response {
    if limited(&door, peer, &headers) {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    let admin = match require_admin(&door, &headers, true).await {
        Ok(admin) => admin,
        Err(refused) => return *refused,
    };
    let Ok(Json(post)) = post else {
        return refusal(StatusCode::BAD_REQUEST, "bad_request", None);
    };
    let (title, body) = match checked(&post.title, &post.body) {
        Ok(checked) => checked,
        Err(problem) => {
            return no_store(
                (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(serde_json::json!({ "error": "invalid_news", "problem": problem.code() })),
                )
                    .into_response(),
            );
        }
    };
    let item = NewsItem {
        id: format!("{:016x}", crate::ticket::random_u64()),
        title,
        body,
        posted: unix_now(),
        author: admin.handle.map(|handle| handle.to_string()),
    };
    let news = door.clone();
    let stored = item.clone();
    match tokio::task::spawn_blocking(move || news.news.post(stored)).await {
        Ok(Ok(())) => {
            tracing::info!(news = %item.id, "admin posted news");
            no_store((StatusCode::CREATED, Json(item)).into_response())
        }
        _ => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(super) async fn delete(
    State(door): State<Arc<Door>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    if limited(&door, peer, &headers) {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    if let Err(refused) = require_admin(&door, &headers, true).await {
        return *refused;
    }
    if id.len() != 16 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let news = door.clone();
    let removed = id.clone();
    match tokio::task::spawn_blocking(move || news.news.delete(&removed)).await {
        Ok(Ok(true)) => {
            tracing::info!(news = %id, "admin deleted news");
            no_store(StatusCode::NO_CONTENT.into_response())
        }
        Ok(Ok(false)) => StatusCode::NOT_FOUND.into_response(),
        _ => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: u64) -> NewsItem {
        NewsItem {
            id: format!("{id:016x}"),
            title: format!("Item {id}"),
            body: "Body".into(),
            posted: id,
            author: None,
        }
    }

    #[test]
    fn titles_and_bodies_are_plain_bounded_text() {
        assert_eq!(
            checked(
                "  Patch 0.2 \u{202E}",
                "Line one\r\nLine two\u{0007}\u{200B}"
            ),
            Ok(("Patch 0.2".into(), "Line one\nLine two".into()))
        );
        // Line breaks belong in bodies only: a space in a title.
        assert_eq!(checked("A\nB", "x").unwrap().0, "A B");
        assert_eq!(
            checked("A\u{2028}B\u{061C}", "x\u{2029}y").unwrap(),
            ("AB".into(), "xy".into())
        );
        assert_eq!(checked(" \u{200B} ", "x"), Err(Problem::EmptyTitle));
        assert_eq!(checked("x", "\n \n"), Err(Problem::EmptyBody));
        assert_eq!(
            checked(&"é".repeat(MAX_TITLE), "x")
                .unwrap()
                .0
                .chars()
                .count(),
            MAX_TITLE
        );
        assert_eq!(
            checked(&"é".repeat(MAX_TITLE + 1), "x"),
            Err(Problem::LongTitle)
        );
        assert_eq!(
            checked("x", &"y".repeat(MAX_BODY + 1)),
            Err(Problem::LongBody)
        );
    }

    #[test]
    fn news_is_newest_first_bounded_and_survives_a_restart() {
        let directory =
            std::env::temp_dir().join(format!("arena-news-{:016x}", crate::ticket::random_u64()));
        let news = News::at(Some(&directory));
        assert!(news.list().is_empty(), "no file yet: no news");
        for id in 1..=(MAX_ITEMS as u64 + 2) {
            news.post(item(id)).unwrap();
        }
        let listed = news.list();
        assert_eq!(listed.len(), MAX_ITEMS);
        assert_eq!(listed[0], item(MAX_ITEMS as u64 + 2), "newest first");
        assert!(
            !listed.contains(&item(1)) && !listed.contains(&item(2)),
            "oldest gone"
        );
        assert!(news.delete(&item(10).id).unwrap());
        assert!(!news.delete(&item(10).id).unwrap());
        // Read back from the file, as after a restart.
        let again = News::at(Some(&directory));
        assert_eq!(again.list(), news.list());
        // A hand-edited file: items failing the checks are dropped, the rest
        // sorted newest first.
        let mut bad = item(1);
        bad.title = "x".repeat(MAX_TITLE + 1);
        let edited = vec![item(3), bad, item(9)];
        std::fs::write(
            directory.join("news.json"),
            serde_json::to_vec(&edited).unwrap(),
        )
        .unwrap();
        assert_eq!(*News::at(Some(&directory)).list(), vec![item(9), item(3)]);
        // A broken file is no news, not a failed start, and is set aside.
        std::fs::write(directory.join("news.json"), "not json").unwrap();
        assert!(News::at(Some(&directory)).list().is_empty());
        assert!(!directory.join("news.json").exists());
        assert!(
            std::fs::read_dir(&directory)
                .unwrap()
                .flatten()
                .any(|entry| entry.file_name().to_string_lossy().contains("unreadable"))
        );
        let _ = std::fs::remove_dir_all(&directory);
        // Without a directory, news lives in memory.
        let memory = News::at(None);
        memory.post(item(1)).unwrap();
        assert_eq!(*memory.list(), vec![item(1)]);
    }
}
