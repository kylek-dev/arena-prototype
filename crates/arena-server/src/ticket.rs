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
use arena_sim::map::MAX_TEAM;
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

const TAG: usize = 32;
/// Shortest accepted signing key.
pub const MIN_KEY: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claims {
    pub match_id: u64,
    pub player: u64,
    pub seat: u8,
    /// Players per team; the match has `2 * team_size` seats.
    pub team_size: u8,
    /// Unix seconds after which the ticket is refused.
    pub expires: u64,
}
impl Claims {
    /// Whether the claims describe a seat in a match the rules can run.
    pub fn is_playable(&self) -> bool {
        (1..=MAX_TEAM).contains(&usize::from(self.team_size)) && self.seat < 2 * self.team_size
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
pub struct TicketKey(Vec<u8>);
impl std::fmt::Debug for TicketKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TicketKey(..)")
    }
}
impl TicketKey {
    /// `None` when shorter than [`MIN_KEY`] bytes.
    pub fn new(bytes: Vec<u8>) -> Option<Self> {
        (bytes.len() >= MIN_KEY).then_some(Self(bytes))
    }
    /// A fresh random key from the operating system.
    pub fn random() -> Self {
        let mut bytes = vec![0; MIN_KEY];
        getrandom::fill(&mut bytes).expect("the operating system provides randomness");
        Self(bytes)
    }
    /// Parses a hex key, as given in `ARENA_TICKET_KEY`.
    pub fn from_hex(text: &str) -> Option<Self> {
        Self::new(from_hex(text.trim())?)
    }

    fn mac(&self) -> Hmac<Sha256> {
        Hmac::<Sha256>::new_from_slice(&self.0).expect("HMAC accepts any key length")
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
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
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
        assert_ne!(TicketKey::random().0, TicketKey::random().0);
        assert_eq!(format!("{:?}", key(1)), "TicketKey(..)", "keys never print");
    }
}
