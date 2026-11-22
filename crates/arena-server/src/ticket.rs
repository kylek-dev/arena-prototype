//! Signed join tickets (ADR 0044, `docs/PROTOCOL.md`).
//!
//! A ticket is the postcard-encoded [`Claims`] followed by a 32-byte
//! HMAC-SHA256 tag over those bytes. The match server derives a connection's
//! seat from the claims alone; the client cannot choose it. A match server
//! starts a match when the first ticket for it arrives, sized by the claims,
//! so whatever holds the key (the front door, or `--dev`) creates matches
//! without talking to the match server. Time is passed in explicitly so
//! verification is deterministic in tests.

use arena_protocol::MAX_TICKET;
use arena_sim::TICKS_PER_SECOND;
use arena_sim::arena::MAX_ROUND_TICKS;
use arena_sim::map::MAX_TEAM;
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

const TAG: usize = 32;
/// Shortest accepted signing key.
pub const MIN_KEY: usize = 32;
/// Current schema version for custom match settings.
pub const MATCH_SETTINGS_VERSION: u8 = 1;
/// Allowed round time limits for custom matches (seconds).
pub const ALLOWED_ROUND_SECONDS: [u16; 4] = [300, 600, 900, 1200];
/// Default custom match round time limit (seconds, matching `MAX_ROUND_TICKS`).
pub const DEFAULT_ROUND_SECONDS: u16 = 1200;

/// Built-in maps a match can run. Never free-form paths or names.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapId {
    #[default]
    Colosseum,
}
impl MapId {
    pub const ALL: [MapId; 1] = [MapId::Colosseum];
    pub fn id(self) -> &'static str {
        match self {
            MapId::Colosseum => "colosseum",
        }
    }
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|map| map.id() == id)
    }
}

/// Versioned, server-enforced match options carried in signed custom tickets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatchSettings {
    pub version: u8,
    pub map: MapId,
    /// Round time limit in seconds; must be in [`ALLOWED_ROUND_SECONDS`].
    pub round_seconds: u16,
}
impl Default for MatchSettings {
    fn default() -> Self {
        Self {
            version: MATCH_SETTINGS_VERSION,
            map: MapId::Colosseum,
            round_seconds: DEFAULT_ROUND_SECONDS,
        }
    }
}
impl MatchSettings {
    /// Whether these settings belong to a supported version and allowlist.
    pub fn is_valid(self) -> bool {
        self.version == MATCH_SETTINGS_VERSION
            && ALLOWED_ROUND_SECONDS.contains(&self.round_seconds)
            && self.round_ticks() <= MAX_ROUND_TICKS
    }
    /// Round time limit in simulation ticks.
    pub fn round_ticks(self) -> u64 {
        u64::from(self.round_seconds) * TICKS_PER_SECOND
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claims {
    pub match_id: u64,
    pub player: u64,
    pub seat: u8,
    /// Players per team; the match has `2 * team_size` seats.
    pub team_size: u8,
    /// Unix seconds after which the ticket is refused.
    pub expires: u64,
    /// Optional custom match settings enforced by the match server.
    pub settings: Option<MatchSettings>,
}
impl Claims {
    /// Whether the claims describe a seat in a match the rules can run.
    pub fn is_playable(&self) -> bool {
        (1..=MAX_TEAM).contains(&usize::from(self.team_size))
            && self.seat < 2 * self.team_size
            && self.settings.is_none_or(MatchSettings::is_valid)
    }
}

/// Join tickets for every seat of one new match.
#[derive(Debug, Serialize, Deserialize)]
pub struct NewMatch {
    pub match_id: u64,
    pub team_size: usize,
    /// Hex-encoded join tickets, one per seat in actor order.
    pub tickets: Vec<String>,
    /// Unix seconds after which the tickets are refused.
    pub expires: u64,
}

/// Issues a ticket per seat for a new match with a random identifier. No
/// match server knows about it until the first ticket connects. `None`
/// when the team size is outside `1..=MAX_TEAM`.
pub fn issue_match(key: &TicketKey, team_size: usize, expires: u64) -> Option<NewMatch> {
    issue_match_with_settings(key, team_size, None, expires)
}

/// Issues a ticket per seat for a custom match with validated versioned
/// settings. Returns `None` when `team_size` or `settings` is invalid.
pub fn issue_custom_match(
    key: &TicketKey,
    team_size: usize,
    settings: MatchSettings,
    expires: u64,
) -> Option<NewMatch> {
    if !settings.is_valid() {
        return None;
    }
    issue_match_with_settings(key, team_size, Some(settings), expires)
}

fn issue_match_with_settings(
    key: &TicketKey,
    team_size: usize,
    settings: Option<MatchSettings>,
    expires: u64,
) -> Option<NewMatch> {
    if !(1..=MAX_TEAM).contains(&team_size) {
        return None;
    }
    let match_id = random_u64();
    let tickets = (0..2 * team_size)
        .map(|seat| {
            to_hex(&key.issue(&Claims {
                match_id,
                player: random_u64(),
                seat: seat as u8,
                team_size: team_size as u8,
                expires,
                settings,
            }))
        })
        .collect();
    Some(NewMatch {
        match_id,
        team_size,
        tickets,
        expires,
    })
}

pub fn random_u64() -> u64 {
    getrandom::u64().expect("the operating system provides randomness")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TicketError {
    Malformed,
    BadSignature,
    Expired,
}

#[derive(Clone)]
pub struct TicketKey(Hmac<Sha256>);
impl std::fmt::Debug for TicketKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TicketKey(..)")
    }
}
impl TicketKey {
    /// `None` when shorter than [`MIN_KEY`] bytes.
    pub fn new(bytes: Vec<u8>) -> Option<Self> {
        if bytes.len() < MIN_KEY {
            return None;
        }
        let mac = Hmac::<Sha256>::new_from_slice(&bytes).expect("HMAC accepts any key length");
        Some(Self(mac))
    }
    /// A fresh random key from the operating system.
    pub fn random() -> Self {
        let mut bytes = vec![0; MIN_KEY];
        getrandom::fill(&mut bytes).expect("the operating system provides randomness");
        Self::new(bytes).expect("MIN_KEY bytes")
    }
    /// Parses a hex key, as given in `ARENA_TICKET_KEY`.
    pub fn from_hex(text: &str) -> Option<Self> {
        Self::new(from_hex(text.trim())?)
    }

    fn mac(&self) -> Hmac<Sha256> {
        self.0.clone()
    }

    pub fn issue(&self, claims: &Claims) -> Vec<u8> {
        let mut ticket = postcard::to_allocvec(claims).expect("claims always encode");
        let mut mac = self.mac();
        mac.update(&ticket);
        ticket.extend_from_slice(&mac.finalize().into_bytes());
        debug_assert!(ticket.len() <= MAX_TICKET);
        ticket
    }

    pub fn verify(&self, ticket: &[u8], now: u64) -> Result<Claims, TicketError> {
        if ticket.len() <= TAG || ticket.len() > MAX_TICKET {
            return Err(TicketError::Malformed);
        }
        let (body, tag) = ticket.split_at(ticket.len() - TAG);
        let mut mac = self.mac();
        mac.update(body);
        // Constant-time comparison; check the signature before parsing.
        mac.verify_slice(tag)
            .map_err(|_| TicketError::BadSignature)?;
        let claims: Claims = match postcard::take_from_bytes(body) {
            Ok((claims, [])) => claims,
            _ => return Err(TicketError::Malformed),
        };
        if now >= claims.expires {
            return Err(TicketError::Expired);
        }
        Ok(claims)
    }
}

pub fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

pub fn from_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) || !text.is_ascii() {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(byte: u8) -> TicketKey {
        TicketKey::new(vec![byte; MIN_KEY]).unwrap()
    }
    const CLAIMS: Claims = Claims {
        match_id: 9,
        player: 77,
        seat: 3,
        team_size: 2,
        expires: 1_000,
        settings: None,
    };

    #[test]
    fn tickets_verify_only_unaltered_unexpired_and_under_their_key() {
        let ticket = key(1).issue(&CLAIMS);
        assert!(ticket.len() <= MAX_TICKET);
        assert_eq!(key(1).verify(&ticket, 999), Ok(CLAIMS));
        assert_eq!(key(1).verify(&ticket, 1_000), Err(TicketError::Expired));
        assert_eq!(key(2).verify(&ticket, 0), Err(TicketError::BadSignature));
        // Flipping any bit anywhere is refused: no seat or match editing.
        for at in 0..ticket.len() {
            let mut forged = ticket.clone();
            forged[at] ^= 1;
            assert!(key(1).verify(&forged, 0).is_err(), "{at}");
        }
        assert_eq!(key(1).verify(&[], 0), Err(TicketError::Malformed));
        // Shorter than a tag, or a body with a byte missing.
        assert_eq!(
            key(1).verify(&ticket[TAG..], 0),
            Err(TicketError::Malformed)
        );
        assert_eq!(
            key(1).verify(&ticket[1..], 0),
            Err(TicketError::BadSignature)
        );
        assert_eq!(
            key(1).verify(&vec![0; MAX_TICKET + 1], 0),
            Err(TicketError::Malformed)
        );
        // A correctly signed body with trailing bytes is still refused.
        let mut body = postcard::to_allocvec(&CLAIMS).unwrap();
        body.push(0);
        let mut mac = key(1).mac();
        mac.update(&body);
        body.extend_from_slice(&mac.finalize().into_bytes());
        assert_eq!(key(1).verify(&body, 0), Err(TicketError::Malformed));
    }

    #[test]
    fn matches_get_one_ticket_per_seat_naming_its_size() {
        assert!(issue_match(&key(1), 0, 10).is_none());
        assert!(issue_match(&key(1), MAX_TEAM + 1, 10).is_none());
        let created = issue_match(&key(1), 3, 10).unwrap();
        assert_eq!(created.tickets.len(), 6);
        for (seat, ticket) in created.tickets.iter().enumerate() {
            let claims = key(1).verify(&from_hex(ticket).unwrap(), 0).unwrap();
            assert_eq!(claims.match_id, created.match_id);
            assert_eq!(
                (claims.seat, claims.team_size, claims.expires),
                (seat as u8, 3, 10)
            );
            assert!(claims.is_playable());
        }
        assert_ne!(
            created.match_id,
            issue_match(&key(1), 3, 10).unwrap().match_id
        );
        let claims = |seat, team_size| Claims {
            seat,
            team_size,
            ..CLAIMS
        };
        assert!(claims(1, 1).is_playable());
        assert!(!claims(2, 1).is_playable(), "seat beyond the match");
        assert!(!claims(0, 0).is_playable());
        assert!(!claims(0, MAX_TEAM as u8 + 1).is_playable());
    }

    #[test]
    fn keys_must_be_long_enough_and_hex_round_trips() {
        assert!(TicketKey::new(vec![0; MIN_KEY - 1]).is_none());
        assert!(TicketKey::from_hex(&"ab".repeat(MIN_KEY)).is_some());
        assert!(TicketKey::from_hex(&"ab".repeat(MIN_KEY - 1)).is_none());
        assert!(TicketKey::from_hex("xyz").is_none());
        assert_eq!(
            from_hex(&to_hex(&[0, 1, 0xfe, 0xff])),
            Some(vec![0, 1, 0xfe, 0xff])
        );
        assert_eq!(from_hex("abc"), None);
        assert_eq!(from_hex("é1"), None);
        assert_ne!(
            TicketKey::random().issue(&CLAIMS),
            TicketKey::random().issue(&CLAIMS)
        );
        assert_eq!(format!("{:?}", key(1)), "TicketKey(..)", "keys never print");
    }

    #[test]
    fn custom_match_settings_are_versioned_bounded_and_signed() {
        let valid = MatchSettings {
            version: MATCH_SETTINGS_VERSION,
            map: MapId::Colosseum,
            round_seconds: 600,
        };
        assert!(valid.is_valid());
        assert_eq!(valid.round_ticks(), 600 * TICKS_PER_SECOND);
        assert_eq!(MapId::from_id("colosseum"), Some(MapId::Colosseum));
        assert_eq!(MapId::from_id("../secret"), None);

        let bad_version = MatchSettings {
            version: MATCH_SETTINGS_VERSION + 1,
            ..valid
        };
        let bad_seconds = MatchSettings {
            round_seconds: 123,
            ..valid
        };
        assert!(!bad_version.is_valid());
        assert!(!bad_seconds.is_valid());
        assert!(issue_custom_match(&key(1), 2, bad_version, 100).is_none());
        assert!(issue_custom_match(&key(1), 2, bad_seconds, 100).is_none());

        let created = issue_custom_match(&key(1), 2, valid, 100).unwrap();
        assert_eq!(created.tickets.len(), 4);
        for (seat, ticket) in created.tickets.iter().enumerate() {
            let bytes = from_hex(ticket).unwrap();
            assert!(bytes.len() <= MAX_TICKET);
            let claims = key(1).verify(&bytes, 0).unwrap();
            assert_eq!(claims.seat, seat as u8);
            assert_eq!(claims.settings, Some(valid));
            assert!(claims.is_playable());
        }
        assert!(
            !Claims {
                settings: Some(bad_seconds),
                ..CLAIMS
            }
            .is_playable()
        );
    }
}
