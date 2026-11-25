//! OAuth providers (ADR 0055): where to send the player, how to trade the
//! code for a token, and how to read the provider's permanent id for them.
//! Minimum scopes: Discord `identify`, Google `openid`, GitHub none.

use serde::Deserialize;

use super::cookie::Login;
use super::store::{MAX_SUBJECT, ProviderId};

/// One configured provider.
#[derive(Clone, Debug)]
pub struct Provider {
    pub id: ProviderId,
    pub client_id: String,
    pub client_secret: String,
    pub authorize_url: String,
    pub token_url: String,
    pub user_url: String,
    pub scope: &'static str,
}

impl Provider {
    /// The provider's real endpoints.
    pub fn new(id: ProviderId, client_id: String, client_secret: String) -> Self {
        let (authorize_url, token_url, user_url, scope) = match id {
            ProviderId::Discord => (
                "https://discord.com/oauth2/authorize",
                "https://discord.com/api/oauth2/token",
                "https://discord.com/api/users/@me",
                "identify",
            ),
            ProviderId::Google => (
                "https://accounts.google.com/o/oauth2/v2/auth",
                "https://oauth2.googleapis.com/token",
                "https://openidconnect.googleapis.com/v1/userinfo",
                "openid",
            ),
            ProviderId::Github => (
                "https://github.com/login/oauth/authorize",
                "https://github.com/login/oauth/access_token",
                "https://api.github.com/user",
                "",
            ),
        };
        Self {
            id,
            client_id,
            client_secret,
            authorize_url: authorize_url.into(),
            token_url: token_url.into(),
            user_url: user_url.into(),
            scope,
        }
    }

    /// The providers configured in the environment: each needs both
    /// `ARENA_OAUTH_<NAME>_CLIENT_ID` and `ARENA_OAUTH_<NAME>_CLIENT_SECRET`.
    pub fn from_env() -> Vec<Self> {
        ProviderId::ALL
            .into_iter()
            .filter_map(|id| {
                let name = id.id().to_ascii_uppercase();
                let value = |suffix: &str| {
                    std::env::var(format!("ARENA_OAUTH_{name}_{suffix}"))
                        .ok()
                        .map(|value| value.trim().to_owned())
                        .filter(|value| !value.is_empty())
                };
                Some(Self::new(id, value("CLIENT_ID")?, value("CLIENT_SECRET")?))
            })
            .collect()
    }

    /// Where to send the browser to log in.
    pub fn authorize(&self, login: &Login, redirect_uri: &str) -> String {
        let mut url = reqwest::Url::parse(&self.authorize_url).expect("provider URLs parse");
        {
            let mut query = url.query_pairs_mut();
            query
                .append_pair("response_type", "code")
                .append_pair("client_id", &self.client_id)
                .append_pair("redirect_uri", redirect_uri)
                .append_pair("state", &login.state)
                .append_pair("code_challenge", &login.challenge())
                .append_pair("code_challenge_method", "S256");
            if !self.scope.is_empty() {
                query.append_pair("scope", self.scope);
            }
        }
        url.into()
    }

    /// Trades the code for the provider's permanent id for this player.
    pub async fn subject(
        &self,
        client: &reqwest::Client,
        code: &str,
        verifier: &str,
        redirect_uri: &str,
    ) -> Result<String, String> {
        #[derive(Deserialize)]
        struct Token {
            access_token: String,
        }
        let token: Token = client
            .post(&self.token_url)
            .header(reqwest::header::ACCEPT, "application/json")
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", redirect_uri),
                ("client_id", &self.client_id),
                ("client_secret", &self.client_secret),
                ("code_verifier", verifier),
            ])
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|error| format!("token request failed: {}", error.without_url()))?
            .json()
            .await
            .map_err(|_| "token response was not understood".to_owned())?;
        let user: serde_json::Value = client
            .get(&self.user_url)
            .bearer_auth(&token.access_token)
            .header(reqwest::header::ACCEPT, "application/json")
            // GitHub refuses requests without a user agent.
            .header(reqwest::header::USER_AGENT, "arena-door")
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|error| format!("user request failed: {}", error.without_url()))?
            .json()
            .await
            .map_err(|_| "user response was not understood".to_owned())?;
        subject_of(self.id, &user).ok_or_else(|| "the provider sent no usable id".to_owned())
    }
}

/// The permanent id in a provider's user response: Discord's `id` (a
/// string), Google's `sub`, GitHub's numeric `id` (never `login`).
pub fn subject_of(provider: ProviderId, user: &serde_json::Value) -> Option<String> {
    let subject = match provider {
        ProviderId::Discord => user.get("id")?.as_str()?.to_owned(),
        ProviderId::Google => user.get("sub")?.as_str()?.to_owned(),
        ProviderId::Github => user.get("id")?.as_u64()?.to_string(),
    };
    (!subject.is_empty()
        && subject.len() <= MAX_SUBJECT
        && subject.bytes().all(|b| b.is_ascii_graphic()))
    .then_some(subject)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn subjects_are_the_permanent_ids() {
        assert_eq!(
            subject_of(
                ProviderId::Discord,
                &json!({"id": "80351110224678912", "username": "x"})
            ),
            Some("80351110224678912".into())
        );
        assert_eq!(
            subject_of(ProviderId::Google, &json!({"sub": "1104"})),
            Some("1104".into())
        );
        assert_eq!(
            subject_of(
                ProviderId::Github,
                &json!({"id": 583231, "login": "octocat"})
            ),
            Some("583231".into())
        );
        // Never a login name, and nothing odd or oversized.
        assert_eq!(
            subject_of(ProviderId::Github, &json!({"login": "octocat"})),
            None
        );
        assert_eq!(
            subject_of(ProviderId::Github, &json!({"id": "583231"})),
            None
        );
        assert_eq!(subject_of(ProviderId::Discord, &json!({"id": ""})), None);
        assert_eq!(subject_of(ProviderId::Google, &json!({"sub": "a b"})), None);
        assert_eq!(
            subject_of(
                ProviderId::Google,
                &json!({"sub": "x".repeat(MAX_SUBJECT + 1)})
            ),
            None
        );
    }

    #[test]
    fn the_authorize_link_asks_for_the_minimum_with_pkce() {
        let login = Login::start(ProviderId::Google, 0);
        let link = Provider::new(ProviderId::Google, "client".into(), "secret".into())
            .authorize(&login, "https://arena.example.com/api/auth/google/callback");
        let url = reqwest::Url::parse(&link).unwrap();
        let pairs: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(pairs["scope"], "openid");
        assert_eq!(pairs["state"], login.state);
        assert_eq!(pairs["code_challenge"], login.challenge());
        assert_eq!(pairs["code_challenge_method"], "S256");
        assert!(
            !link.contains("secret"),
            "the secret never travels in a link"
        );
        assert!(
            !link.contains(&login.verifier),
            "only the challenge travels"
        );
        let github =
            Provider::new(ProviderId::Github, "c".into(), "s".into()).authorize(&login, "x");
        assert!(!github.contains("scope="), "GitHub needs no scope");
    }
}
