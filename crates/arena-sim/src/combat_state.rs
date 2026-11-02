//! Per-player casting timers. Call `tick()` once per simulation tick.
//! Uses the newer spells.rs with GcdType and calculate_gcd_ticks.
//!
//! GCD starts when a cast begins. Personal cooldown starts on successful
//! completion, or immediately for an instant spell. Interrupting keeps the
//! existing GCD and does not start the interrupted spell's cooldown.
//!
//! casting.rs must check resources, targets, range, LoS and crowd control
//! before calling start_cast, retain the cast's target/context, and resolve
//! any completion returned by tick or start_cast.

use std::collections::HashMap;

use crate::spells::{calculate_gcd_ticks, GcdType, Spell, SpellId, SpellType};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActiveCast {
    pub spell_id: SpellId,
    pub remaining_ticks: u32,
    // Snapshot the duration so later definition changes cannot alter this cast.
    cooldown_ticks: u32,
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
    cooldowns: HashMap<SpellId, u32>,
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
            cooldowns: HashMap::new(),
            active_cast: None,
            haste_percentage: 0,
            gcd_multiplier_percentage: 100,
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
            GcdType::Normal => calculate_gcd_ticks(
                self.haste_percentage,
                self.gcd_multiplier_percentage,
            ),
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