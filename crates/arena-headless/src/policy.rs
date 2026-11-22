//! Deterministic bot RNG and combat decision policy shared between
//! `arena-headless` (zero-network balance simulations) and `arena-bot`
//! (networked server stress runs).

use std::time::Duration;

use arena_sim::arena::{ATTACK_RANGE, ActorInput, Axes, CastAction, Combatant};
use arena_sim::spells::{
    PULSE_COST, QUELL_COST, QUELL_RANGE, RIFT_BOLT_COST, RIFT_BOLT_RANGE, SpellId,
};

/// Human-like reaction delay (ticks, ~0.47 s at 30 Hz) before attempting an
/// off-GCD `Quell` interrupt on a casting enemy so cast bars remain readable.
pub const QUELL_REACTION_TICKS: u32 = 14;

/// Small deterministic generator (`xorshift64*`), so headless and bot runs
/// can be repeated bit-for-bit from the same seed.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        // Never zero, and different seeds diverge immediately.
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform in `[0, 1)`.
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Uniform duration in `[0, limit)`.
    pub fn below(&mut self, limit: Duration) -> Duration {
        limit.mul_f64(self.unit())
    }
}

/// Computes one tick of bot intent for `actor` from authoritative fighter
/// state. Bots chase the nearest standing enemy, strafe at close range, cast
/// timed `RiftBolt` and instant `Pulse`, interrupt casting enemies with
/// off-GCD `Quell`, face their target, and jump occasionally.
///
/// The rules judge every action (range, line of sight, cooldowns, resources);
/// bots deliberately omit LOS checks so pillar blocks and interrupts occur
/// naturally in play.
pub fn decide(actor: usize, tick: u64, fighters: &[Combatant], rng: &mut Rng) -> ActorInput {
    let Some(me) = fighters.get(actor) else {
        return ActorInput::default();
    };
    if me.health == 0 {
        return ActorInput::default();
    }
    let target = fighters
        .iter()
        .enumerate()
        .filter(|(index, fighter)| index % 2 != actor % 2 && fighter.health > 0)
        .min_by_key(|(_, fighter)| me.position.distance_squared(fighter.position));
    let Some((index, enemy)) = target else {
        return ActorInput::default();
    };

    let dx = f64::from(enemy.position.x) - f64::from(me.position.x);
    let dz = f64::from(enemy.position.z) - f64::from(me.position.z);
    let distance = dx.hypot(dz).max(1.0);
    // Planar heading: 0 faces −Z, counter-clockwise from above.
    let yaw = (-dx).atan2(-dz);
    let facing = ((yaw / std::f64::consts::TAU).rem_euclid(1.0) * 65536.0).round() as u32 as u16;

    // While actively casting, stand still so timed casts complete cleanly.
    if me.active_cast.is_some() {
        return ActorInput {
            movement: Some(Axes { x: 0, z: 0 }),
            facing: Some(facing),
            jump: false,
            action: None,
            rewind_ticks: 0,
        };
    }

    let (ux, uz) = (dx / distance, dz / distance);
    let close = distance < f64::from(ATTACK_RANGE) * 0.8;
    let movement = if close {
        // Orbit/strafe around the target instead of standing frozen during GCDs or lockouts.
        let sign = if (actor + (tick / 45) as usize).is_multiple_of(2) {
            1.0
        } else {
            -1.0
        };
        let (mut sx, mut sz) = (-uz * sign, ux * sign);
        if distance < 2_500.0 {
            sx -= ux * 0.35;
            sz -= uz * 0.35;
            let len = sx.hypot(sz).max(1e-6);
            sx /= len;
            sz /= len;
        }
        Axes {
            x: (sx * 1000.0) as i16,
            z: (sz * 1000.0) as i16,
        }
    } else {
        // Wander a little so bots round pillars instead of sticking to them.
        let wobble = (rng.unit() - 0.5) * 0.6;
        let (sin, cos) = wobble.sin_cos();
        let (mx, mz) = (ux * cos - uz * sin, ux * sin + uz * cos);
        Axes {
            x: (mx * 1000.0) as i16,
            z: (mz * 1000.0) as i16,
        }
    };

    // Interrupt a casting enemy within Quell range after a reaction window.
    let casting_enemy = fighters
        .iter()
        .enumerate()
        .find(|(idx, fighter)| {
            idx % 2 != actor % 2
                && fighter.health > 0
                && fighter.active_cast.is_some_and(|cast| {
                    cast.total_ticks.saturating_sub(cast.remaining_ticks) >= QUELL_REACTION_TICKS
                })
                && me.position.distance_squared(fighter.position) <= i64::from(QUELL_RANGE).pow(2)
        })
        .map(|(idx, _)| idx);
    if let Some(victim) = casting_enemy
        && me.lockout_remaining == 0
        && me.cooldowns[SpellId::Quell.index()] == 0
        && me.resource >= QUELL_COST
    {
        return ActorInput {
            movement: Some(movement),
            facing: Some(facing),
            jump: false,
            action: Some(CastAction::Cast {
                spell: SpellId::Quell,
                target: Some(victim),
            }),
            rewind_ticks: 0,
        };
    }

    // Stagger Rift Bolt casts across mid-range and combat so bots weave visible timed casts.
    let can_bolt = distance <= f64::from(RIFT_BOLT_RANGE)
        && me.height == 0
        && me.lockout_remaining == 0
        && me.gcd_remaining == 0
        && me.cooldowns[SpellId::RiftBolt.index()] == 0
        && me.resource >= RIFT_BOLT_COST;
    let bolt_chance = if distance > f64::from(ATTACK_RANGE) {
        0.06
    } else {
        0.04
    };
    if can_bolt && rng.unit() < bolt_chance {
        return ActorInput {
            movement: Some(Axes { x: 0, z: 0 }),
            facing: Some(facing),
            jump: false,
            action: Some(CastAction::Cast {
                spell: SpellId::RiftBolt,
                target: Some(index),
            }),
            rewind_ticks: 0,
        };
    }

    let can_pulse = distance <= f64::from(ATTACK_RANGE)
        && me.lockout_remaining == 0
        && me.gcd_remaining == 0
        && me.cooldowns[SpellId::Pulse.index()] == 0
        && me.resource >= PULSE_COST;
    ActorInput {
        movement: Some(movement),
        facing: Some(facing),
        jump: rng.unit() < 1.0 / 90.0,
        action: if can_pulse {
            ActorInput::pulse(index)
        } else {
            None
        },
        rewind_ticks: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arena_sim::arena::{ActiveCastState, Position};

    fn fighter(x: i32, z: i32, health: u16) -> Combatant {
        let mut c = Combatant::new(Position { x, z }, 0);
        c.health = health;
        c
    }

    #[test]
    fn rng_is_deterministic_and_bounded() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..64 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
        assert_ne!(Rng::new(42).next_u64(), Rng::new(43).next_u64());
        let mut rng = Rng::new(7);
        assert!((0..10_000).all(|_| (0.0..1.0).contains(&rng.unit())));
        assert_eq!(rng.below(Duration::ZERO), Duration::ZERO);
    }

    #[test]
    fn dead_or_missing_actor_is_idle() {
        let mut rng = Rng::new(1);
        let fighters = [fighter(0, 0, 0), fighter(3_000, 0, 100)];
        assert_eq!(decide(0, 1, &fighters, &mut rng), ActorInput::default());
        assert_eq!(decide(5, 1, &fighters, &mut rng), ActorInput::default());
    }

    #[test]
    fn active_cast_stands_still_and_quell_waits_for_reaction_window() {
        let mut rng = Rng::new(1);
        let mut caster = fighter(0, 0, 100);
        caster.active_cast = Some(ActiveCastState {
            spell: SpellId::RiftBolt,
            target: Some(1),
            remaining_ticks: 20,
            total_ticks: 45,
        });
        let fighters = [caster, fighter(8_000, 0, 100)];
        let input = decide(0, 1, &fighters, &mut rng);
        assert_eq!(input.movement, Some(Axes { x: 0, z: 0 }));
        assert_eq!(input.action, None);

        // Enemy just started casting (elapsed = 2 < 14): do not kick yet.
        let mut enemy_early = fighter(6_000, 0, 100);
        enemy_early.active_cast = Some(ActiveCastState {
            spell: SpellId::RiftBolt,
            target: Some(0),
            remaining_ticks: 43,
            total_ticks: 45,
        });
        let early = decide(0, 1, &[fighter(0, 0, 100), enemy_early], &mut rng);
        assert_ne!(
            early.action,
            Some(CastAction::Cast {
                spell: SpellId::Quell,
                target: Some(1),
            })
        );

        // Enemy past reaction window (elapsed = 25 >= 14): kick with Quell.
        let mut enemy_late = fighter(6_000, 0, 100);
        enemy_late.active_cast = Some(ActiveCastState {
            spell: SpellId::RiftBolt,
            target: Some(0),
            remaining_ticks: 20,
            total_ticks: 45,
        });
        let late = decide(0, 1, &[fighter(0, 0, 100), enemy_late], &mut rng);
        assert_eq!(
            late.action,
            Some(CastAction::Cast {
                spell: SpellId::Quell,
                target: Some(1),
            })
        );
    }
}
