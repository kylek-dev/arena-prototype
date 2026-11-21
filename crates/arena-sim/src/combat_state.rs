//! Per-player casting timers. Call `tick()` once per simulation tick.
//! Uses `spells.rs` with `GcdType` and `calculate_gcd_ticks`.
//!
//! GCD starts when a cast begins. Personal cooldown starts on successful
//! completion, or immediately for an instant spell. Interrupting keeps the
//! existing GCD and does not start the interrupted spell's cooldown.
//!
//! `casting.rs` must check resources, targets, range, LoS and crowd control
//! before calling `start_cast`, retain the cast's target/context, and resolve
//! any completion returned by `tick` or `start_cast`.

use std::collections::BTreeMap;

use crate::spells::{GcdType, Spell, SpellId, SpellType, calculate_gcd_ticks};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActiveCast {
    pub spell_id: SpellId,
    pub remaining_ticks: u32,
    pub total_ticks: u32,
    // Snapshot the duration so later definition changes cannot alter this cast.
    cooldown_ticks: u32,
}

impl ActiveCast {
    pub const fn new(
        spell_id: SpellId,
        remaining_ticks: u32,
        total_ticks: u32,
        cooldown_ticks: u32,
    ) -> Self {
        Self {
            spell_id,
            remaining_ticks,
            total_ticks,
            cooldown_ticks,
        }
    }

    pub const fn cooldown_ticks(&self) -> u32 {
        self.cooldown_ticks
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CastStateError {
    PassiveSpell,
    AlreadyCasting,
    GlobalCooldown { remaining_ticks: u32 },
    SpellCooldown { remaining_ticks: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CastStart {
    // The caller can resolve this spell immediately.
    Instant(SpellId),
    // Resolve the spell when tick() returns its ID.
    Casting,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CombatState {
    gcd_remaining: u32,
    cooldowns: BTreeMap<SpellId, u32>,
    active_cast: Option<ActiveCast>,
    haste_percentage: u32,
    gcd_multiplier_percentage: u32,
}

impl Default for CombatState {
    fn default() -> Self {
        Self::new()
    }
}

impl CombatState {
    pub fn new() -> Self {
        Self {
            gcd_remaining: 0,
            cooldowns: BTreeMap::new(),
            active_cast: None,
            haste_percentage: 0,
            gcd_multiplier_percentage: 100,
        }
    }

    /// Reconstructs combat timers from authoritative fighter fields.
    pub fn from_parts(
        gcd_remaining: u32,
        cooldowns: BTreeMap<SpellId, u32>,
        active_cast: Option<ActiveCast>,
        haste_percentage: u32,
        gcd_multiplier_percentage: u32,
    ) -> Self {
        let cooldowns = cooldowns
            .into_iter()
            .filter(|(_, ticks)| *ticks > 0)
            .collect();
        Self {
            gcd_remaining,
            cooldowns,
            active_cast,
            haste_percentage,
            gcd_multiplier_percentage,
        }
    }

    // Use whole percentages: 5 means 5% haste.
    // Stat changes affect future GCDs, not a timer already running.
    pub fn set_haste_percentage(&mut self, percentage: u32) {
        self.haste_percentage = percentage;
    }

    // 100 = normal GCD; 50 = half GCD for a class/passive that grants it.
    // This only changes Normal GCDs. Custom durations are exact tick values.
    pub fn set_gcd_multiplier_percentage(&mut self, percentage: u32) {
        self.gcd_multiplier_percentage = percentage;
    }

    pub fn gcd_remaining(&self) -> u32 {
        self.gcd_remaining
    }

    pub fn cooldown_remaining(&self, spell_id: SpellId) -> u32 {
        self.cooldowns.get(&spell_id).copied().unwrap_or(0)
    }

    pub fn active_cast(&self) -> Option<&ActiveCast> {
        self.active_cast.as_ref()
    }

    pub fn is_casting(&self) -> bool {
        self.active_cast.is_some()
    }

    pub fn gcd_ticks_for(&self, spell: &Spell) -> u32 {
        if spell.spell_type == SpellType::Passive {
            return 0;
        }

        match &spell.gcd {
            GcdType::Normal => {
                calculate_gcd_ticks(self.haste_percentage, self.gcd_multiplier_percentage)
            }
            GcdType::OffGcd => 0,
            GcdType::Custom(ticks) => *ticks,
        }
    }

    // Checks timing only. Other casting rules belong in casting.rs.
    // OffGcd bypasses GCD, but does not allow simultaneous casts.
    pub fn can_cast(&self, spell: &Spell) -> Result<(), CastStateError> {
        if spell.spell_type == SpellType::Passive {
            return Err(CastStateError::PassiveSpell);
        }

        if self.is_casting() {
            return Err(CastStateError::AlreadyCasting);
        }

        let remaining_ticks = self.cooldown_remaining(spell.id);
        if remaining_ticks > 0 {
            return Err(CastStateError::SpellCooldown { remaining_ticks });
        }

        if !matches!(&spell.gcd, GcdType::OffGcd) && self.gcd_remaining > 0 {
            return Err(CastStateError::GlobalCooldown {
                remaining_ticks: self.gcd_remaining,
            });
        }

        Ok(())
    }

    // Call after casting.rs has accepted resources, conditions and targeting.
    // A rejected attempt leaves every timer unchanged.
    pub fn start_cast(&mut self, spell: &Spell) -> Result<CastStart, CastStateError> {
        self.can_cast(spell)?;

        if !matches!(&spell.gcd, GcdType::OffGcd) {
            self.gcd_remaining = self.gcd_ticks_for(spell);
        }

        if spell.cast_ticks == 0 {
            self.start_personal_cooldown(spell.id, spell.cooldown_ticks);
            return Ok(CastStart::Instant(spell.id));
        }

        self.active_cast = Some(ActiveCast {
            spell_id: spell.id,
            remaining_ticks: spell.cast_ticks,
            total_ticks: spell.cast_ticks,
            cooldown_ticks: spell.cooldown_ticks,
        });

        Ok(CastStart::Casting)
    }

    // Advance exactly one simulation tick. At 30 Hz, call 30 times per second.
    // A completed cast is returned once. Its personal cooldown begins here.
    pub fn tick(&mut self) -> Option<SpellId> {
        self.gcd_remaining = self.gcd_remaining.saturating_sub(1);

        self.cooldowns.retain(|_, remaining| {
            *remaining = remaining.saturating_sub(1);
            *remaining > 0
        });

        let finished = match self.active_cast.as_mut() {
            Some(cast) => {
                cast.remaining_ticks = cast.remaining_ticks.saturating_sub(1);
                if cast.remaining_ticks == 0 {
                    Some(*cast)
                } else {
                    None
                }
            }
            None => None,
        };

        if let Some(cast) = finished {
            self.active_cast = None;
            self.start_personal_cooldown(cast.spell_id, cast.cooldown_ticks);
            return Some(cast.spell_id);
        }

        None
    }

    // Casting/CC code can use the returned ID to discard its pending context.
    // Existing GCD and cooldowns keep running.
    pub fn interrupt_cast(&mut self) -> Option<SpellId> {
        self.active_cast.take().map(|cast| cast.spell_id)
    }

    pub fn reset_spell_cooldown(&mut self, spell_id: SpellId) {
        self.cooldowns.remove(&spell_id);
    }

    // Start a new round with clean timers, keeping the player's stat settings.
    pub fn reset(&mut self) {
        self.gcd_remaining = 0;
        self.cooldowns.clear();
        self.active_cast = None;
    }

    fn start_personal_cooldown(&mut self, spell_id: SpellId, ticks: u32) {
        if ticks > 0 {
            self.cooldowns.insert(spell_id, ticks);
        } else {
            self.cooldowns.remove(&spell_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spells::{ARENA_CLASS, RIFT_BOLT_CAST_TICKS, RIFT_BOLT_COOLDOWN_TICKS};

    #[test]
    fn instant_spell_starts_gcd_and_personal_cooldown_while_off_gcd_bypasses_gcd() {
        let mut combat = CombatState::new();
        combat.set_haste_percentage(23);
        let pulse = SpellId::Pulse.definition();
        let bolt = SpellId::RiftBolt.definition();
        let quell = SpellId::Quell.definition();

        assert_eq!(combat.gcd_ticks_for(&pulse), 29);
        assert_eq!(
            combat.start_cast(&pulse),
            Ok(CastStart::Instant(SpellId::Pulse))
        );
        assert_eq!(combat.gcd_remaining(), 29);
        assert_eq!(
            combat.cooldown_remaining(SpellId::Pulse),
            pulse.cooldown_ticks
        );

        // Personal cooldown blocks Pulse; GCD blocks RiftBolt; OffGcd Quell still works.
        assert_eq!(
            combat.can_cast(&pulse),
            Err(CastStateError::SpellCooldown {
                remaining_ticks: pulse.cooldown_ticks
            })
        );
        assert_eq!(
            combat.can_cast(&bolt),
            Err(CastStateError::GlobalCooldown {
                remaining_ticks: 29
            })
        );
        assert_eq!(
            combat.start_cast(&quell),
            Ok(CastStart::Instant(SpellId::Quell))
        );
        assert_eq!(
            combat.gcd_remaining(),
            29,
            "OffGcd does not alter running GCD"
        );
        assert_eq!(
            combat.cooldown_remaining(SpellId::Quell),
            quell.cooldown_ticks
        );

        // Passive spells are always rejected without modifying timers.
        let passive = Spell::passive(SpellId::Mend, "Passive", ARENA_CLASS, Vec::new());
        assert_eq!(
            combat.start_cast(&passive),
            Err(CastStateError::PassiveSpell)
        );
        assert_eq!(combat.gcd_ticks_for(&passive), 0);
    }

    #[test]
    fn timed_cast_starts_personal_cooldown_on_completion_not_on_interrupt() {
        let mut combat = CombatState::new();
        combat.set_haste_percentage(23);
        let bolt = SpellId::RiftBolt.definition();
        let quell = SpellId::Quell.definition();

        assert_eq!(combat.start_cast(&bolt), Ok(CastStart::Casting));
        assert!(combat.is_casting());
        assert_eq!(combat.cooldown_remaining(SpellId::RiftBolt), 0);
        // Even OffGcd cannot cast concurrently with an active cast.
        assert_eq!(combat.can_cast(&quell), Err(CastStateError::AlreadyCasting));

        // Interrupting keeps the running GCD and does not start personal cooldown.
        assert_eq!(combat.interrupt_cast(), Some(SpellId::RiftBolt));
        assert!(!combat.is_casting());
        assert_eq!(combat.cooldown_remaining(SpellId::RiftBolt), 0);
        assert_eq!(combat.gcd_remaining(), 29);

        // Wait out GCD, then cast to completion.
        for _ in 0..29 {
            assert_eq!(combat.tick(), None);
        }
        assert_eq!(combat.start_cast(&bolt), Ok(CastStart::Casting));
        for _ in 1..RIFT_BOLT_CAST_TICKS {
            assert_eq!(combat.tick(), None);
        }
        assert_eq!(combat.tick(), Some(SpellId::RiftBolt));
        assert!(!combat.is_casting());
        assert_eq!(
            combat.cooldown_remaining(SpellId::RiftBolt),
            RIFT_BOLT_COOLDOWN_TICKS
        );

        combat.reset_spell_cooldown(SpellId::RiftBolt);
        assert_eq!(combat.cooldown_remaining(SpellId::RiftBolt), 0);

        // Reset clears timers while preserving haste/GCD multiplier settings.
        combat.set_gcd_multiplier_percentage(50);
        let half_gcd = combat.gcd_ticks_for(&bolt);
        combat.reset();
        assert_eq!(combat.gcd_remaining(), 0);
        assert_eq!(combat.gcd_ticks_for(&bolt), half_gcd);
    }

    #[test]
    fn custom_gcd_uses_exact_ticks_independent_of_haste() {
        let mut combat = CombatState::new();
        combat.set_haste_percentage(50);
        combat.set_gcd_multiplier_percentage(50);
        let mut custom = Spell::active(SpellId::Pulse, "Custom", ARENA_CLASS);
        custom.gcd = GcdType::Custom(12);
        assert_eq!(combat.gcd_ticks_for(&custom), 12);
    }
}
