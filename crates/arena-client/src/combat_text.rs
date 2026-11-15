//! Outgoing, non-self health changes only. Positive deltas are healing.
#[derive(Default)]
pub(super) struct CombatText {
    next_id: u64,
    pub entries: Vec<Entry>,
}

pub(super) struct Entry {
    pub id: u64,
    pub target: usize,
    pub delta: i32,
    pub age: f32,
}

impl CombatText {
    pub fn record(&mut self, source: usize, target: usize, delta: i32) {
        // Practice actor 0 is local. Never infer attribution from health snapshots.
        if source != 0 || target == 0 || delta == 0 {
            return;
        }
        self.next_id += 1;
        if self.entries.len() == 32 {
            self.entries.remove(0);
        }
        self.entries.push(Entry {
            id: self.next_id,
            target,
            delta,
            age: 0.0,
        });
    }

    pub fn advance(&mut self, seconds: f32) {
        for entry in &mut self.entries {
            entry.age += seconds;
        }
        self.entries.retain(|entry| entry.age < 1.2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_own_outgoing_damage_and_healing_survive_until_expiry() {
        let mut text = CombatText::default();
        for delta in [-25, 30] {
            text.record(1, 0, delta);
            text.record(1, 2, delta);
            text.record(0, 0, delta);
            text.record(0, 1, delta);
        }
        text.record(0, 1, 0);
        assert_eq!(text.entries.len(), 2);
        assert_eq!(text.entries[0].delta, -25);
        assert_eq!(text.entries[1].delta, 30);
        assert_ne!(text.entries[0].id, text.entries[1].id);
        text.advance(0.6);
        assert_eq!(text.entries.len(), 2);
        text.advance(0.6);
        assert!(text.entries.is_empty());
        text.record(0, 1, -25);
        text = CombatText::default();
        assert!(text.entries.is_empty());
    }

    #[test]
    fn bursts_are_bounded() {
        let mut text = CombatText::default();
        for _ in 0..100 {
            text.record(0, 1, -1);
        }
        assert_eq!(text.entries.len(), 32);
        assert_eq!(text.entries[0].id, 69);
    }
}
