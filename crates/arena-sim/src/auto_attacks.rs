use crate::TICKS_PER_SECOND;
use crate::arena::Combatant;
use crate::map::Layout;

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
    // Supply the arena's current layout for its line-of-sight rules.
    pub fn try_swing(
        &mut self,
        tick: u64,
        attacker: &Combatant,
        selected_enemy: Option<&Combatant>,
        is_casting: bool,
        layout: &Layout,
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

        if !layout.line_of_sight(attacker.position, enemy.position) {
            return None;
        }

        // Only a successful swing starts a new cooldown.
        self.ready_at = tick.saturating_add(SWING_INTERVAL_TICKS);

        Some(SWING_DAMAGE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arena::Position;
    use crate::map::Pillar;

    fn fighter(x: i32) -> Combatant {
        Combatant::new(Position { x, z: 0 }, 0)
    }

    #[test]
    fn swing_uses_the_active_map_and_blocked_attempts_keep_the_timer_ready() {
        let mut layout = Layout {
            half_x: 5_000,
            half_z: 5_000,
            pillars: vec![Pillar {
                center: Position { x: 0, z: 0 },
                half_x: 100,
                half_z: 100,
            }],
            rooms: Vec::new(),
        };

        let attacker = fighter(-1_000);
        let enemy = fighter(1_000);
        let mut auto = AutoAttackState::default();

        assert_eq!(
            auto.try_swing(0, &attacker, Some(&enemy), false, &layout),
            None
        );

        layout.pillars.clear();

        assert_eq!(
            auto.try_swing(0, &attacker, Some(&enemy), false, &layout),
            Some(SWING_DAMAGE)
        );
    }
}
