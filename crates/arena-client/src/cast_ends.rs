//! How each unit's timed cast just ended, for the unit frames' cast bars:
//! completed, cancelled (the caster stopped it, moved, lost range, sight or
//! its target) or interrupted (by an enemy's interrupt or a lockout). Taken
//! from the authoritative events only, never inferred from snapshots.
use arena_sim::arena::InterruptReason;
use arena_sim::spells::SpellId;

/// How long an ending stays published: the page plays it once per id.
const LIFETIME: f32 = 1.2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Outcome {
    Completed,
    Cancelled,
    Interrupted,
}

impl Outcome {
    pub fn interrupted(reason: InterruptReason) -> Self {
        match reason {
            InterruptReason::Interrupted | InterruptReason::Silenced => Self::Interrupted,
            InterruptReason::Cancelled
            | InterruptReason::Moved
            | InterruptReason::OutOfRange
            | InterruptReason::LineOfSight
            | InterruptReason::TargetDead
            | InterruptReason::CasterDead => Self::Cancelled,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
        }
    }
}

#[derive(Default)]
pub(super) struct CastEnds {
    next_id: u64,
    pub entries: Vec<Entry>,
}

pub(super) struct Entry {
    pub id: u64,
    pub actor: usize,
    pub spell: SpellId,
    pub outcome: Outcome,
    pub age: f32,
}

impl CastEnds {
    /// Records how a timed cast ended. Instant spells have no cast bar to
    /// end, though an instant interrupt (Quell) reports `CastCompleted`.
    pub fn record(&mut self, actor: usize, spell: SpellId, outcome: Outcome) {
        if spell.definition().cast_ticks == 0 {
            return;
        }
        self.next_id += 1;
        if self.entries.len() == 32 {
            self.entries.remove(0);
        }
        self.entries.push(Entry {
            id: self.next_id,
            actor,
            spell,
            outcome,
            age: 0.0,
        });
    }

    pub fn advance(&mut self, seconds: f32) {
        for entry in &mut self.entries {
            entry.age += seconds;
        }
        self.entries.retain(|entry| entry.age < LIFETIME);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interrupts_and_lockouts_read_as_interrupted_and_the_rest_as_cancelled() {
        assert_eq!(
            Outcome::interrupted(InterruptReason::Interrupted),
            Outcome::Interrupted
        );
        assert_eq!(
            Outcome::interrupted(InterruptReason::Silenced),
            Outcome::Interrupted
        );
        for reason in [
            InterruptReason::Cancelled,
            InterruptReason::Moved,
            InterruptReason::OutOfRange,
            InterruptReason::LineOfSight,
            InterruptReason::TargetDead,
            InterruptReason::CasterDead,
        ] {
            assert_eq!(Outcome::interrupted(reason), Outcome::Cancelled);
        }
    }

    #[test]
    fn endings_get_new_ids_and_expire() {
        let mut ends = CastEnds::default();
        ends.record(0, SpellId::RiftBolt, Outcome::Completed);
        ends.record(3, SpellId::Mend, Outcome::Interrupted);
        assert_eq!(ends.entries.len(), 2);
        assert_ne!(ends.entries[0].id, ends.entries[1].id);
        ends.advance(0.6);
        assert_eq!(ends.entries.len(), 2);
        ends.advance(0.6);
        assert!(ends.entries.is_empty());
    }

    #[test]
    fn instant_spells_have_no_cast_bar_to_end() {
        let mut ends = CastEnds::default();
        // Quell, an instant interrupt, reports CastCompleted.
        ends.record(1, SpellId::Quell, Outcome::Completed);
        ends.record(1, SpellId::Pulse, Outcome::Completed);
        assert!(ends.entries.is_empty());
        ends.record(1, SpellId::Mend, Outcome::Completed);
        assert_eq!(ends.entries.len(), 1);
    }

    #[test]
    fn bursts_are_bounded() {
        let mut ends = CastEnds::default();
        for _ in 0..100 {
            ends.record(1, SpellId::RiftBolt, Outcome::Cancelled);
        }
        assert_eq!(ends.entries.len(), 32);
        assert_eq!(ends.entries[0].id, 69);
    }
}
