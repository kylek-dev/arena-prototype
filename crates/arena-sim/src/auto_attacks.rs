use crate::TICKS_PER_SECOND;
use crate::arena::{Combatant, line_of_sight};

pub const MELEE_RANGE_MM: i32 = 2_000;
pub const SWING_INTERVAL_TICKS: u64 = 2 * TICKS_PER_SECOND;
pub const SWING_DAMAGE: u16 = 2;

// Keep one of these per fighter. The first swing is ready immediately.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AutoAttackState {
    ready_at: u64,
}

impl AutoAttackState {
    // Call once per simulation tick after processing abilities.
    // Pass None when the fighter has no valid enemy selected.
    // Pass true for an active cast or an ability used on this tick.
    pub fn try_swing(
        &mut self,
        tick: u64,
        attacker: &Combatant,
        selected_enemy: Option<&Combatant>,
        is_casting: bool,
    ) -> Option<u16> {
        if is_casting || attacker.health == 0 || tick < self.ready_at {
            return None;
        }

        let enemy = selected_enemy?;

        if enemy.health == 0 {
            return None;
        }

        let distance_squared = attacker.position.distance_squared(enemy.position);
        if distance_squared > i64::from(MELEE_RANGE_MM).pow(2) {
            return None;
        }

        if !line_of_sight(attacker.position, enemy.position) {
            return None;
        }

        // Only a successful swing starts a new cooldown.
        self.ready_at = tick.saturating_add(SWING_INTERVAL_TICKS);

        Some(SWING_DAMAGE)
    }
}