//! Choosing a `Name#1234` handle (social-identity step 3): what a name may
//! be. The protocol (`arena_protocol::Handle`) fixes the character set and
//! length; this adds reserved and blocked words. The number is the store's
//! job.

use arena_protocol::{Handle, MAX_HANDLE_NAME, MIN_HANDLE_NAME};

const BUILT_IN: &str = include_str!("blocked-names.txt");

/// Why a name was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameRefusal {
    /// Not 3–16 ASCII letters and digits starting with a letter.
    Invalid,
    /// Contains a reserved or blocked word.
    Blocked,
}

impl NameRefusal {
    pub fn code(self) -> &'static str {
        match self {
            NameRefusal::Invalid => "invalid_name",
            NameRefusal::Blocked => "blocked_name",
        }
    }
}

/// The words names may not contain.
#[derive(Clone, Debug)]
pub struct Blocklist(Vec<String>);

impl Blocklist {
    /// The built-in list plus the lines of `extra` (same format).
    pub fn new(extra: &str) -> Self {
        let words = [BUILT_IN, extra]
            .iter()
            .flat_map(|text| text.lines())
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(fold)
            .filter(|word| !word.is_empty())
            .collect();
        Self(words)
    }

    /// The built-in list, plus the file named by `ARENA_BLOCKED_NAMES`.
    pub fn from_env() -> Result<Self, String> {
        match std::env::var("ARENA_BLOCKED_NAMES") {
            Ok(path) if !path.trim().is_empty() => std::fs::read_to_string(path.trim())
                .map(|extra| Self::new(&extra))
                .map_err(|error| format!("ARENA_BLOCKED_NAMES {path}: {error}")),
            _ => Ok(Self::new("")),
        }
    }

    /// Checks a chosen name; the number comes later.
    pub fn check(&self, name: &str) -> Result<(), NameRefusal> {
        if !(MIN_HANDLE_NAME..=MAX_HANDLE_NAME).contains(&name.len())
            || Handle::new(name, 1).is_none()
        {
            return Err(NameRefusal::Invalid);
        }
        // `1` stands for `i` or `l`: check both readings.
        let readings = [fold(name), fold(&name.replace('1', "l"))];
        if readings
            .iter()
            .any(|folded| self.0.iter().any(|word| folded.contains(word.as_str())))
        {
            return Err(NameRefusal::Blocked);
        }
        Ok(())
    }
}

/// Lowercase, with digits that stand for letters swapped back and other
/// digits dropped, so `Adm1n` and `4dmin` read as `admin`.
fn fold(text: &str) -> String {
    text.chars()
        .filter_map(|c| match c.to_ascii_lowercase() {
            '0' => Some('o'),
            '1' => Some('i'),
            '3' => Some('e'),
            '4' => Some('a'),
            '5' => Some('s'),
            '7' => Some('t'),
            '8' => Some('b'),
            c if c.is_ascii_lowercase() => Some(c),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_follow_the_protocol_and_avoid_blocked_words() {
        let list = Blocklist::new("");
        for fine in [
            "Rook",
            "Gladiator7",
            "Ashen",
            "Grapevine",
            "Spicy",
            "Classic",
        ] {
            assert_eq!(list.check(fine), Ok(()), "{fine}");
        }
        for (bad, why) in [
            ("ab", NameRefusal::Invalid),
            ("Rook Seven", NameRefusal::Invalid),
            ("7Rook", NameRefusal::Invalid),
            ("Ad_min", NameRefusal::Invalid),
            ("Admin", NameRefusal::Blocked),
            ("Adm1n", NameRefusal::Blocked),
            ("TheAdmin", NameRefusal::Blocked),
            ("Guest", NameRefusal::Blocked),
            ("ArenaStaff", NameRefusal::Blocked),
            ("SH1Tlord", NameRefusal::Blocked),
        ] {
            assert_eq!(list.check(bad), Err(why), "{bad}");
        }
    }

    #[test]
    fn substring_matching_trades_false_positives_for_simplicity() {
        let list = Blocklist::new("");
        // Both readings of `1` are checked.
        assert_eq!(list.check("Hit1er"), Err(NameRefusal::Blocked));
        assert_eq!(list.check("Sh1t"), Err(NameRefusal::Blocked));
        // Known innocent names refused because they contain a blocked word;
        // operators can rename players by hand until an allowlist exists.
        for innocent in ["Badminton", "Quarterstaff", "Scunthorpe"] {
            assert_eq!(
                list.check(innocent),
                Err(NameRefusal::Blocked),
                "{innocent}"
            );
        }
    }

    #[test]
    fn operators_extend_the_list() {
        let list = Blocklist::new("# ours\nmagistrate\n\n");
        assert_eq!(list.check("TheMagistrate"), Err(NameRefusal::Blocked));
        assert_eq!(Blocklist::new("").check("TheMagistrate"), Ok(()));
    }
}
