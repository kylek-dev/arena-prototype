//! WorkOS AuthKit (ADR 0055): players log in on WorkOS's hosted page with
//! email and password, an emailed code, Google or GitHub. The door sends the
//! browser there with state and PKCE, trades the returned code for the
//! WorkOS user id, and keeps nothing else WorkOS knows (no email, name or
//! token). <https://workos.com/docs/reference/authkit/authentication>

use serde::Deserialize;

use super::cookie::{Login, base64url_decode};
use super::store::MAX_SUBJECT;

/// The real API; tests point at a fake.
pub const API: &str = "https://api.workos.com";

/// The door's WorkOS application.
#[derive(Clone)]
pub struct WorkOs {
    /// `client_...`, public.
    pub client_id: String,
    /// `sk_...`, secret: never logged, never in a link.
    pub api_key: String,
    pub api: String,
}

impl std::fmt::Debug for WorkOs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkOs")
            .field("client_id", &self.client_id)
            .field("api", &self.api)
            .finish_non_exhaustive()
    }
}

/// Who logged in: the WorkOS user, and the WorkOS session to end at
/// logout (absent if WorkOS sent none we could read).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoggedIn {
    pub user: String,
    pub session: Option<String>,
}

/// Whether a WorkOS id (`user_01H...`, `session_01H...`) is plausible:
/// short, and only letters, digits and `_`.
fn plausible(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_SUBJECT
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

impl WorkOs {
    /// From `ARENA_WORKOS_CLIENT_ID` and `ARENA_WORKOS_API_KEY`.
    pub fn from_env() -> Result<Self, String> {
        let value = |name: &str| {
            std::env::var(name)
                .ok()
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| format!("accounts need {name}"))
        };
        Ok(Self {
            client_id: value("ARENA_WORKOS_CLIENT_ID")?,
            api_key: value("ARENA_WORKOS_API_KEY")?,
            api: API.into(),
        })
    }

    /// Where to send the browser to log in (or, with `sign_up`, to make an
    /// account) on the hosted page.
    pub fn authorize(&self, login: &Login, redirect_uri: &str, sign_up: bool) -> String {
        let mut url = reqwest::Url::parse(&format!("{}/user_management/authorize", self.api))
            .expect("the API URL parses");
        {
            let mut query = url.query_pairs_mut();
            query
                .append_pair("response_type", "code")
                .append_pair("client_id", &self.client_id)
                .append_pair("redirect_uri", redirect_uri)
                .append_pair("provider", "authkit")
                .append_pair("state", &login.state)
                .append_pair("code_challenge", &login.challenge())
                .append_pair("code_challenge_method", "S256");
            if sign_up {
                query.append_pair("screen_hint", "sign-up");
            }
        }
        url.into()
    }

    /// Trades the code (with the PKCE verifier) for who logged in.
    pub async fn authenticate(
        &self,
        client: &reqwest::Client,
        code: &str,
        verifier: &str,
    ) -> Result<LoggedIn, String> {
        #[derive(Deserialize)]
        struct User {
            id: String,
        }
        #[derive(Deserialize)]
        struct Authenticated {
            user: User,
            access_token: Option<String>,
        }
        let answer: Authenticated = client
            .post(format!("{}/user_management/authenticate", self.api))
            .json(&serde_json::json!({
                "client_id": self.client_id,
                "client_secret": self.api_key,
                "grant_type": "authorization_code",
                "code": code,
                "code_verifier": verifier,
            }))
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|error| format!("authenticate failed: {}", error.without_url()))?
            .json()
            .await
            .map_err(|_| "authenticate response was not understood".to_owned())?;
        if !plausible(&answer.user.id) {
            return Err("WorkOS sent no usable user id".into());
        }
        Ok(LoggedIn {
            user: answer.user.id,
            session: answer.access_token.as_deref().and_then(session_of),
        })
    }

    /// Where to send the browser to end its WorkOS session, then return to
    /// `return_to` (a sign-out URI configured in the dashboard).
    pub fn logout(&self, session: &str, return_to: &str) -> String {
        let mut url = reqwest::Url::parse(&format!("{}/user_management/sessions/logout", self.api))
            .expect("the API URL parses");
        url.query_pairs_mut()
            .append_pair("session_id", session)
            .append_pair("return_to", return_to);
        url.into()
    }

    /// Deletes the user at WorkOS (their email and login methods). A user
    /// WorkOS no longer has counts as deleted.
    pub async fn delete_user(&self, client: &reqwest::Client, user: &str) -> Result<(), String> {
        if !plausible(user) {
            return Err("not a WorkOS user id".into());
        }
        let response = client
            .delete(format!("{}/user_management/users/{user}", self.api))
            .bearer_auth(&self.api_key)
            .send()
            .await
            .map_err(|error| format!("delete failed: {}", error.without_url()))?;
        let status = response.status();
        if status.is_success() || status == reqwest::StatusCode::NOT_FOUND {
            Ok(())
        } else {
            Err(format!("delete refused: {status}"))
        }
    }
}

/// The `sid` claim of a WorkOS access token: the session to end at logout.
/// The token came straight from WorkOS over TLS in answer to our own
/// authenticated request, and only this id is read from it, so its
/// signature is not checked here.
pub fn session_of(access_token: &str) -> Option<String> {
    let payload = access_token.split('.').nth(1)?;
    let claims: serde_json::Value = serde_json::from_slice(&base64url_decode(payload)?).ok()?;
    let sid = claims.get("sid")?.as_str()?;
    plausible(sid).then(|| sid.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::cookie::base64url;

    fn workos() -> WorkOs {
        WorkOs {
            client_id: "client_123".into(),
            api_key: "sk_test_secret".into(),
            api: API.into(),
        }
    }

    #[test]
    fn the_login_link_uses_authkit_with_pkce_and_no_secret() {
        let login = Login::start(0);
        let link = workos().authorize(&login, "https://arena.example.com/api/auth/callback", false);
        let url = reqwest::Url::parse(&link).unwrap();
        assert_eq!(url.path(), "/user_management/authorize");
        let pairs: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(pairs["provider"], "authkit");
        assert_eq!(pairs["client_id"], "client_123");
        assert_eq!(pairs["state"], login.state);
        assert_eq!(pairs["code_challenge"], login.challenge());
        assert_eq!(pairs["code_challenge_method"], "S256");
        assert!(!pairs.contains_key("screen_hint"));
        assert!(!link.contains("sk_test"), "the key never travels in a link");
        assert!(
            !link.contains(&login.verifier),
            "only the challenge travels"
        );
        let sign_up = workos().authorize(&login, "x", true);
        assert!(sign_up.contains("screen_hint=sign-up"));
    }

    #[test]
    fn the_session_id_comes_from_the_access_token() {
        let token = |claims: serde_json::Value| {
            format!(
                "{}.{}.sig",
                base64url(br#"{"alg":"RS256"}"#),
                base64url(claims.to_string().as_bytes())
            )
        };
        assert_eq!(
            session_of(&token(
                serde_json::json!({"sid": "session_01HQ", "sub": "user_1"})
            ))
            .as_deref(),
            Some("session_01HQ")
        );
        assert_eq!(
            session_of(&token(serde_json::json!({"sub": "user_1"}))),
            None
        );
        assert_eq!(
            session_of(&token(serde_json::json!({"sid": "a&b=c"}))),
            None,
            "nothing that could break a link"
        );
        assert_eq!(session_of("not a token"), None);
    }

    #[test]
    fn logout_links_name_the_session_and_return_page() {
        let link = workos().logout("session_01HQ", "https://arena.example.com/");
        let url = reqwest::Url::parse(&link).unwrap();
        assert_eq!(url.path(), "/user_management/sessions/logout");
        let pairs: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(pairs["session_id"], "session_01HQ");
        assert_eq!(pairs["return_to"], "https://arena.example.com/");
    }
}
