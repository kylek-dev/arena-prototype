//! Signed cookie values (ADR 0055): the session and the short-lived OAuth
//! login state, both HMAC-SHA256 under `ARENA_SESSION_KEY` with a purpose
//! prefix so one can never be replayed as the other.

use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};

use crate::ticket::{from_hex, to_hex};

/// The session cookie. `__Host-` makes browsers require `Secure`, `Path=/`
/// and no `Domain`, so a sibling subdomain cannot set or read it.
pub const SESSION_COOKIE: &str = "__Host-arena_session";
/// The login-in-progress cookie: state and PKCE verifier.
pub const OAUTH_COOKIE: &str = "__Host-arena_oauth";
/// How long a login session lasts.
pub const SESSION_SECONDS: u64 = 30 * 24 * 60 * 60;
/// How long a started login may take.
pub const OAUTH_SECONDS: u64 = 10 * 60;
/// Shortest accepted key.
pub const MIN_KEY: usize = 32;

#[derive(Clone)]
pub struct SessionKey(Hmac<Sha256>);

impl std::fmt::Debug for SessionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SessionKey(..)")
    }
}

impl SessionKey {
    /// `None` when not hex or shorter than [`MIN_KEY`] bytes.
    pub fn from_hex(text: &str) -> Option<Self> {
        let bytes = from_hex(text.trim())?;
        (bytes.len() >= MIN_KEY)
            .then(|| Hmac::new_from_slice(&bytes).ok().map(Self))
            .flatten()
    }

    pub fn random() -> Self {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).expect("the operating system provides randomness");
        Self(Hmac::new_from_slice(&bytes).expect("any length works"))
    }

    fn tag(&self, purpose: &str, body: &str) -> String {
        let mut mac = self.0.clone();
        mac.update(purpose.as_bytes());
        mac.update(b"\n");
        mac.update(body.as_bytes());
        to_hex(&mac.finalize().into_bytes())
    }

    fn sign(&self, purpose: &str, body: String) -> String {
        let tag = self.tag(purpose, &body);
        format!("{body}.{tag}")
    }

    /// The body of a value signed for `purpose`, when the tag matches.
    fn open<'a>(&self, purpose: &str, value: &'a str) -> Option<&'a str> {
        let (body, tag) = value.rsplit_once('.')?;
        let tag = from_hex(tag)?;
        let mut mac = self.0.clone();
        mac.update(purpose.as_bytes());
        mac.update(b"\n");
        mac.update(body.as_bytes());
        mac.verify_slice(&tag).ok().map(|()| body)
    }

    pub fn session(&self, session: &Session) -> String {
        self.sign(
            "session",
            format!(
                "2.{:016x}.{}.{}.{}",
                session.account,
                session.generation,
                session.expires,
                session.workos.as_deref().unwrap_or("-")
            ),
        )
    }

    /// A signed, unexpired session cookie's contents. The account must
    /// still be checked against the store.
    pub fn read_session(&self, value: &str, now: u64) -> Option<Session> {
        let body = self.open("session", value)?;
        let mut parts = body.split('.');
        let (Some("2"), Some(account), Some(generation), Some(expires), Some(workos), None) = (
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
        ) else {
            return None;
        };
        let session = Session {
            account: u64::from_str_radix(account, 16)
                .ok()
                .filter(|id| *id != 0)?,
            generation: generation.parse().ok()?,
            expires: expires.parse().ok()?,
            workos: (workos != "-").then(|| workos.to_owned()),
        };
        (now < session.expires).then_some(session)
    }

    pub fn login(&self, login: &Login) -> String {
        self.sign(
            "oauth",
            format!("2.{}.{}.{}", login.state, login.verifier, login.expires),
        )
    }

    /// A signed, unexpired login-in-progress cookie's contents.
    pub fn read_login(&self, value: &str, now: u64) -> Option<Login> {
        let body = self.open("oauth", value)?;
        let mut parts = body.split('.');
        let (Some("2"), Some(state), Some(verifier), Some(expires), None) = (
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
        ) else {
            return None;
        };
        let login = Login {
            state: state.to_owned(),
            verifier: verifier.to_owned(),
            expires: expires.parse().ok()?,
        };
        (now < login.expires).then_some(login)
    }
}

/// A logged-in browser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    pub account: u64,
    pub generation: u32,
    pub expires: u64,
    /// The WorkOS session (`session_...`) to end at logout. WorkOS ids
    /// hold only letters, digits and `_`, never the `.` separator.
    pub workos: Option<String>,
}

/// A login in progress: what the callback must see again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Login {
    /// Random, URL-safe; must come back unchanged from the provider.
    pub state: String,
    /// The PKCE code verifier; its S256 challenge went to the provider.
    pub verifier: String,
    pub expires: u64,
}

impl Login {
    pub fn start(now: u64) -> Self {
        Self {
            state: random_token(),
            verifier: random_token(),
            expires: now + OAUTH_SECONDS,
        }
    }

    /// The S256 PKCE challenge for this login's verifier (RFC 7636).
    pub fn challenge(&self) -> String {
        pkce_challenge(&self.verifier)
    }
}

/// 32 random bytes, base64url: a state or a PKCE verifier (43 characters,
/// within RFC 7636's 43..=128).
pub fn random_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the operating system provides randomness");
    base64url(&bytes)
}

pub fn pkce_challenge(verifier: &str) -> String {
    base64url(&Sha256::digest(verifier.as_bytes()))
}

/// Base64url without padding (RFC 4648 §5).
pub fn base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..=chunk.len() {
            out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
    }
    out
}

/// Decodes base64url, with or without padding; `None` on anything else.
pub fn base64url_decode(text: &str) -> Option<Vec<u8>> {
    let text = text.trim_end_matches('=');
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let mut bits = 0u32;
    let mut held = 0;
    for byte in text.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return None,
        };
        bits = (bits << 6) | u32::from(value);
        held += 6;
        if held >= 8 {
            held -= 8;
            out.push((bits >> held) as u8);
        }
    }
    // A single leftover character cannot encode a byte.
    (held < 6).then_some(out)
}

/// The value of cookie `name` in a `Cookie` header.
pub fn cookie<'a>(header: &'a str, name: &str) -> Option<&'a str> {
    header.split(';').find_map(|pair| {
        let (key, value) = pair.trim().split_once('=')?;
        (key == name).then_some(value)
    })
}

/// A `Set-Cookie` value for a `__Host-` cookie lasting `seconds` (0 clears).
pub fn set_cookie(name: &str, value: &str, seconds: u64) -> String {
    format!("{name}={value}; Path=/; Max-Age={seconds}; HttpOnly; Secure; SameSite=Lax")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> SessionKey {
        SessionKey::from_hex(&"ab".repeat(32)).unwrap()
    }

    #[test]
    fn sessions_verify_only_unaltered_unexpired_and_under_their_key() {
        let session = Session {
            account: 0xdead_beef,
            generation: 2,
            expires: 1_000,
            workos: Some("session_01HQ".into()),
        };
        let value = key().session(&session);
        assert_eq!(key().read_session(&value, 999), Some(session.clone()));
        let without = Session {
            workos: None,
            ..session.clone()
        };
        assert_eq!(
            key().read_session(&key().session(&without), 0),
            Some(without)
        );
        assert_eq!(key().read_session(&value, 1_000), None, "expired");
        assert_eq!(
            SessionKey::random().read_session(&value, 0),
            None,
            "other key"
        );
        for at in 0..value.len() {
            let mut forged = value.clone().into_bytes();
            forged[at] = if forged[at] == b'1' { b'2' } else { b'1' };
            let forged = String::from_utf8(forged).unwrap();
            if forged != value {
                assert_eq!(key().read_session(&forged, 0), None, "{forged}");
            }
        }
        // A login cookie can never pass as a session, nor the reverse.
        let login = key().login(&Login::start(0));
        assert_eq!(key().read_session(&login, 0), None);
        assert_eq!(key().read_login(&value, 0), None);
        assert!(SessionKey::from_hex("abcd").is_none(), "too short");
        assert!(SessionKey::from_hex(&"zz".repeat(32)).is_none(), "not hex");
    }

    #[test]
    fn logins_round_trip_and_expire() {
        let login = Login::start(100);
        let value = key().login(&login);
        assert_eq!(
            key().read_login(&value, 100 + OAUTH_SECONDS - 1),
            Some(login.clone())
        );
        assert_eq!(key().read_login(&value, 100 + OAUTH_SECONDS), None);
        assert_eq!(login.state.len(), 43);
        assert_ne!(login.state, login.verifier);
    }

    #[test]
    fn pkce_matches_rfc_7636_appendix_b() {
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        assert_eq!(base64url(b"f"), "Zg");
        assert_eq!(base64url(b"fo"), "Zm8");
        assert_eq!(base64url(b"foo"), "Zm9v");
        for bytes in [&b""[..], b"f", b"fo", b"foo", b"\xff\xfe\x00"] {
            assert_eq!(base64url_decode(&base64url(bytes)).as_deref(), Some(bytes));
        }
        assert_eq!(base64url_decode("Zm8=").as_deref(), Some(&b"fo"[..]));
        assert_eq!(base64url_decode("Z"), None);
        assert_eq!(base64url_decode("Zm+v"), None, "base64, not base64url");
    }

    #[test]
    fn cookies_are_found_by_exact_name() {
        let header = "a=1; __Host-arena_session=v.1; __Host-arena_sessionx=2";
        assert_eq!(cookie(header, SESSION_COOKIE), Some("v.1"));
        assert_eq!(cookie(header, "b"), None);
        assert_eq!(
            set_cookie(SESSION_COOKIE, "v", 0),
            "__Host-arena_session=v; Path=/; Max-Age=0; HttpOnly; Secure; SameSite=Lax"
        );
    }
}
