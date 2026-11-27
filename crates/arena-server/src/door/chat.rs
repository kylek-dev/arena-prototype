//! Chat (planning/issues/chat.md): messages travel over the queue
//! WebSocket every game page already holds, and the door decides who hears
//! them. The client sends only text and the channel it wants; the door sets
//! the sender (`Name#1234` and admin tag) and refuses what breaks the rules.
//!
//! Channels:
//! - `public`: everyone connected to this door.
//! - `match`: the players of the sender's current match.
//! - `team`: the sender's team in that match, or else their custom lobby or
//!   party.
//! - `spectator`: the spectators of the sender's current match. Players
//!   never receive it, and spectators post nowhere else (no ghosting).
//! - `whisper`: to a friend by handle (logged-in players only).
//!
//! `system` is reserved for messages the server or the page itself writes;
//! no client can post to it.

use tokio::time::{Duration, Instant};

/// Longest message, in characters.
pub const MAX_CHAT: usize = 200;
/// Messages one connection may send at once, then one per [`CHAT_EVERY`].
pub const CHAT_BURST: u32 = 5;
pub const CHAT_EVERY: Duration = Duration::from_secs(1);

/// A channel a client may post to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    Public,
    Match,
    Team,
    Spectator,
    /// The speaker's hub instance (ADR 0056).
    Hub,
}

impl Channel {
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "public" => Some(Self::Public),
            "match" => Some(Self::Match),
            "team" => Some(Self::Team),
            "spectator" => Some(Self::Spectator),
            "hub" => Some(Self::Hub),
            _ => None,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Match => "match",
            Self::Team => "team",
            Self::Spectator => "spectator",
            Self::Hub => "hub",
        }
    }
}

/// Combining marks kept on one character (enough for any script; more is
/// "Zalgo" text spilling over other lines).
const MAX_MARKS: usize = 3;

/// Invisible or layout characters: zero-width ones, bidi overrides and
/// isolates (which reorder what follows) and the byte-order mark.
fn invisible(c: char) -> bool {
    matches!(c, '\u{061C}' | '\u{180E}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{206F}' | '\u{FEFF}')
}

fn combining(c: char) -> bool {
    matches!(c, '\u{0300}'..='\u{036F}' | '\u{1AB0}'..='\u{1AFF}' | '\u{1DC0}'..='\u{1DFF}' | '\u{20D0}'..='\u{20FF}' | '\u{FE20}'..='\u{FE2F}')
}

/// Chat text as delivered: control characters and line or paragraph
/// separators become spaces (one message, one line, so nothing can pass
/// for a second, system-looking line), invisible characters are dropped,
/// combining marks stop at [`MAX_MARKS`] per character, the ends are
/// trimmed, and it must hold 1–[`MAX_CHAT`] characters. Masking blocked
/// words is the caller's next step.
pub fn clean(text: &str) -> Result<String, &'static str> {
    let mut cleaned = String::with_capacity(text.len());
    let mut marks = 0;
    for c in text.chars() {
        if invisible(c) {
            continue;
        }
        if combining(c) {
            marks += 1;
            if marks <= MAX_MARKS {
                cleaned.push(c);
            }
            continue;
        }
        marks = 0;
        let line_break = matches!(c, '\u{2028}' | '\u{2029}');
        cleaned.push(if c.is_control() || line_break { ' ' } else { c });
    }
    let cleaned = cleaned.trim();
    match cleaned.chars().count() {
        0 => Err("empty"),
        n if n > MAX_CHAT => Err("too_long"),
        _ => Ok(cleaned.to_owned()),
    }
}

/// A connection's chat allowance: [`CHAT_BURST`] at once, refilling one
/// every [`CHAT_EVERY`].
#[derive(Debug)]
pub struct ChatBudget {
    tokens: u32,
    refill_at: Instant,
}

impl ChatBudget {
    pub fn new(now: Instant) -> Self {
        Self {
            tokens: CHAT_BURST,
            refill_at: now + CHAT_EVERY,
        }
    }

    pub fn take(&mut self, now: Instant) -> bool {
        while self.tokens < CHAT_BURST && now >= self.refill_at {
            self.tokens += 1;
            self.refill_at += CHAT_EVERY;
        }
        if self.tokens == CHAT_BURST {
            self.refill_at = now + CHAT_EVERY;
        }
        if self.tokens == 0 {
            return false;
        }
        self.tokens -= 1;
        true
    }
}

/// Where a connection last took a seat, for `match`, `team` and
/// `spectator` chat: the match, its team (`None` for a spectator), and when.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChatSeat {
    pub match_id: u64,
    pub team: Option<u8>,
    pub at: Instant,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_cleaned_and_bounded() {
        assert_eq!(clean("  gg wp \n"), Ok("gg wp".into()));
        assert_eq!(clean("a\u{7}b"), Ok("a b".into()));
        assert_eq!(clean(" \t "), Err("empty"));
        assert_eq!(clean(&"é".repeat(MAX_CHAT)), Ok("é".repeat(MAX_CHAT)));
        assert_eq!(clean(&"é".repeat(MAX_CHAT + 1)), Err("too_long"));
        // Invisible and reordering characters go; line separators become
        // spaces; stacked marks stop at three.
        assert_eq!(clean("g\u{200B}g \u{202E}pw"), Ok("gg pw".into()));
        assert_eq!(
            clean("hi\u{2028}[System] you won"),
            Ok("hi [System] you won".into())
        );
        assert_eq!(
            clean("a\u{301}\u{301}\u{301}\u{301}\u{301}b"),
            Ok("a\u{301}\u{301}\u{301}b".into())
        );
        assert_eq!(clean("\u{FEFF}\u{200D}"), Err("empty"));
    }

    #[test]
    fn the_budget_allows_a_burst_then_one_a_second() {
        let start = Instant::now();
        let mut budget = ChatBudget::new(start);
        for _ in 0..CHAT_BURST {
            assert!(budget.take(start));
        }
        assert!(!budget.take(start), "flood refused");
        assert!(budget.take(start + CHAT_EVERY));
        assert!(!budget.take(start + CHAT_EVERY));
    }

    #[test]
    fn only_four_channels_are_open_to_clients() {
        for id in ["public", "match", "team", "spectator"] {
            assert_eq!(Channel::from_id(id).map(Channel::id), Some(id));
        }
        for id in ["system", "whisper", "Public", ""] {
            assert_eq!(Channel::from_id(id), None, "{id}");
        }
    }
}
