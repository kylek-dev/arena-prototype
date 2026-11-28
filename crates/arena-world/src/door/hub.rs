//! Why a player is no longer in the hub (ADR 0057): the door's word for
//! leaving a hub world, as `hub_left {reason}` says it. The hub itself is a
//! world on the world server; the door places players (`door/worlds.rs`).

/// Why a player is no longer in the hub, as `hub_left` says it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Left {
    /// They asked to leave, or their world connection ended.
    Left,
    /// A match (or spectator) ticket took them into a match.
    Match,
}
impl Left {
    pub fn id(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Match => "match",
        }
    }
}
