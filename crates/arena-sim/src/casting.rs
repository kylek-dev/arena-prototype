use std::collections::HashMap;

use crate::classes::{Class, Resource};
use crate::combat_state::{CastStart, CastStateError, CombatState};
use crate::spells::{Spell, SpellCondition, SpellId, SpellTarget};

// Use your arena's stable unit ID type here.
pub type UnitId = usize;

// Borrow the player's actual values, so paying costs updates the player.
pub struct CasterContext<'a> {
    pub id: UnitId,
    pub class: Class,
    pub health: &'a mut u16,
    pub max_health: u16,
    pub resources: &'a mut HashMap<Resource, u16>,
    pub known_spells: &'a [SpellId],
    pub active_effects: &'a [SpellId],
    pub is_stunned: bool,

    // True only when the current silence blocks THIS spell.
    // The arena supplies this using the spell's school/exemption rules.
    pub silence_blocks_spell: bool,

    pub is_moving: bool,
}

// Supply a fresh view from the arena when checking a target.
#[derive(Clone, Copy)]
pub struct TargetContext<'a> {
    pub id: UnitId,
    pub is_friendly: bool,
    pub health: u16,
    pub max_health: u16,

    // Use the same distance units as Spell.range, squared.
    pub distance_squared: u64,

    pub has_line_of_sight: bool,
    pub is_stunned: bool,
    pub active_effects: &'a [SpellId],
}

// Returned when a spell is ready for effects.rs to resolve.
#[derive(Clone, Debug)]
pub struct PreparedCast {
    pub spell: Spell,
    pub caster_id: UnitId,
    pub target_id: Option<UnitId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CastError {
    Timing(CastStateError),
    WrongClass,
    UnknownSpell,
    CasterDead,
    InvalidHealth,
    Stunned,
    Silenced,
    Moving,
    InvalidTarget,
    TargetDead,
    OutOfRange,
    NoLineOfSight,
    ConditionNotMet,
    NotEnoughHealth,
    NotEnoughResource(Resource),
    InvalidResourceCost,
    StateMismatch,
}

#[derive(Default, Debug)]
pub struct CastingState {
    pending: Option<PreparedCast>,
}

impl CastingState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn pending_cast(&self) -> Option<&PreparedCast> {
        self.pending.as_ref()
    }

    pub fn pending_target_id(&self) -> Option<UnitId> {
        self.pending.as_ref().and_then(|cast| cast.target_id)
    }

    // Some = instant spell ready to resolve.
    // None = timed cast started.
    // Validation happens before resources or timers are changed.
    pub fn try_cast(
        &mut self,
        combat: &mut CombatState,
        caster: &mut CasterContext<'_>,
        spell: &Spell,
        target: Option<&TargetContext<'_>>,
    ) -> Result<Option<PreparedCast>, CastError> {
        if self.pending.is_some() {
            return Err(CastError::Timing(CastStateError::AlreadyCasting));
        }

     let remaining_ticks = combat.cooldown_remaining(spell.id);

if !cooldown_is_ready(remaining_ticks) {
    return Err(CastError::Timing(
        CastStateError::SpellCooldown {
            remaining_ticks,
        },
    ));
}

        let target_id = validate_cast(spell, caster, target)?;

        // Combine repeated costs for the same resource.
        let mut costs = HashMap::<Resource, u32>::new();

        for cost in &spell.resource_costs {
            let total = costs.entry(cost.resource).or_insert(0);

            *total = total
                .checked_add(u32::from(cost.amount))
                .ok_or(CastError::InvalidResourceCost)?;
        }

        for (resource, amount) in &costs {
            let available = caster
                .resources
                .get(resource)
                .copied()
                .unwrap_or(0);

            if u32::from(available) < *amount {
                return Err(CastError::NotEnoughResource(*resource));
            }
        }

        // HP costs must leave at least 1 HP.
        if spell.health_cost >= *caster.health && spell.health_cost > 0 {
            return Err(CastError::NotEnoughHealth);
        }

        let prepared = PreparedCast {
            spell: spell.clone(),
            caster_id: caster.id,
            target_id,
        };

        let started = combat
            .start_cast(spell)
            .map_err(CastError::Timing)?;

        // Pay once, at cast start.
        // Interruptions do not refund these costs.
        *caster.health -= spell.health_cost;

        for (resource, amount) in costs {
            if let Some(current) = caster.resources.get_mut(&resource) {
                *current -= amount as u16;
            }
        }

        match started {
            CastStart::Instant(_) => Ok(Some(prepared)),

            CastStart::Casting => {
                self.pending = Some(prepared);
                Ok(None)
            }
        }
    }

    // Call once per simulation tick.
    // This also advances CombatState.
    //
    // Look up pending_target_id() in the arena instead of using
    // whichever target the player currently has selected.
    pub fn tick(
        &mut self,
        combat: &mut CombatState,
        caster: &CasterContext<'_>,
        target: Option<&TargetContext<'_>>,
    ) -> Result<Option<PreparedCast>, CastError> {
        if self.pending.is_none() && combat.is_casting() {
            self.interrupt_cast(combat);
            let _ = combat.tick();

            return Err(CastError::StateMismatch);
        }

        if let Some(pending) = self.pending.as_ref() {
            let active = combat.active_cast();

            if active.map(|cast| cast.spell_id) != Some(pending.spell.id)
                || pending.caster_id != caster.id
            {
                self.interrupt_cast(combat);
                let _ = combat.tick();

                return Err(CastError::StateMismatch);
            }

            // These can interrupt the cast immediately.
            let mut validation = check_caster(&pending.spell, caster);

            // Recheck the target and conditions before completion.
            if validation.is_ok() && active.unwrap().remaining_ticks <= 1 {
                validation = validate_cast(&pending.spell, caster, target)
                    .and_then(|target_id| {
                        if target_id == pending.target_id {
                            Ok(())
                        } else {
                            Err(CastError::InvalidTarget)
                        }
                    });
            }

            if let Err(error) = validation {
                self.interrupt_cast(combat);
                let _ = combat.tick();

                return Err(error);
            }
        }

        match combat.tick() {
            Some(id) => {
                let finished = self
                    .pending
                    .take()
                    .ok_or(CastError::StateMismatch)?;

                if finished.spell.id != id {
                    return Err(CastError::StateMismatch);
                }

                Ok(Some(finished))
            }

            None => Ok(None),
        }
    }

    // Route interrupts through here to keep both states synchronized.
    pub fn interrupt_cast(
        &mut self,
        combat: &mut CombatState,
    ) -> Option<SpellId> {
        self.pending = None;
        combat.interrupt_cast()
    }

    pub fn reset(&mut self, combat: &mut CombatState) {
        self.pending = None;
        combat.reset();
    }
}

fn check_caster(
    spell: &Spell,
    caster: &CasterContext<'_>,
) -> Result<(), CastError> {
    if *caster.health == 0 {
        return Err(CastError::CasterDead);
    }

    if caster.max_health == 0 {
        return Err(CastError::InvalidHealth);
    }

    if caster.is_stunned {
        return Err(CastError::Stunned);
    }

    if caster.silence_blocks_spell {
        return Err(CastError::Silenced);
    }

    if caster.is_moving
        && spell.cast_ticks > 0
        && !spell.can_cast_while_moving
    {
        return Err(CastError::Moving);
    }

    Ok(())
}



pub fn cooldown_is_ready(remaining_ticks: u32) -> bool {
    if remaining_ticks > 0 {
        return false;
    }

    return true;
}

fn validate_cast(
    spell: &Spell,
    caster: &CasterContext<'_>,
    target: Option<&TargetContext<'_>>,
) -> Result<Option<UnitId>, CastError> {
    check_caster(spell, caster)?;

    if spell.class != caster.class {
        return Err(CastError::WrongClass);
    }

    if !caster.known_spells.contains(&spell.id) {
        return Err(CastError::UnknownSpell);
    }

    let self_target = TargetContext {
        id: caster.id,
        is_friendly: true,
        health: *caster.health,
        max_health: caster.max_health,
        distance_squared: 0,
        has_line_of_sight: true,
        is_stunned: caster.is_stunned,
        active_effects: caster.active_effects,
    };

    let resolved = match spell.target {
        SpellTarget::NoTarget => None,

        SpellTarget::SelfTarget => Some(self_target),

        SpellTarget::Enemy | SpellTarget::Friendly => {
            let requested = target.ok_or(CastError::InvalidTarget)?;

            let target = if requested.id == caster.id {
                self_target
            } else {
                *requested
            };

            let needs_friendly = spell.target == SpellTarget::Friendly;

            if target.is_friendly != needs_friendly {
                return Err(CastError::InvalidTarget);
            }

            Some(target)
        }
    };

    if let Some(target) = resolved.as_ref() {
        if target.health == 0 {
            return Err(CastError::TargetDead);
        }

        if target.max_health == 0 {
            return Err(CastError::InvalidHealth);
        }

        if target.id != caster.id {
            let range = u64::from(spell.range);

            if target.distance_squared > range * range {
                return Err(CastError::OutOfRange);
            }

            if !target.has_line_of_sight {
                return Err(CastError::NoLineOfSight);
            }
        }
    }

    let conditions_met = spell.conditions.iter().all(|condition| {
        condition_met(condition, caster, resolved.as_ref())
    });

    if !conditions_met {
        return Err(CastError::ConditionNotMet);
    }

    Ok(resolved.map(|target| target.id))
}

fn condition_met(
    condition: &SpellCondition,
    caster: &CasterContext<'_>,
    target: Option<&TargetContext<'_>>,
) -> bool {
    match condition {
        SpellCondition::TargetHasEffect(id) => {
            target.map_or(false, |target| {
                target.active_effects.contains(id)
            })
        }

        SpellCondition::TargetDoesNotHaveEffect(id) => {
            target.map_or(false, |target| {
                !target.active_effects.contains(id)
            })
        }

        SpellCondition::CasterHasEffect(id) => {
            caster.active_effects.contains(id)
        }

        // Strict comparisons without rounding the health percentage.
        SpellCondition::TargetHealthAbove(percentage) => {
            target.map_or(false, |target| {
                u32::from(target.health) * 100
                    > u32::from(target.max_health)
                        * u32::from(*percentage)
            })
        }

        SpellCondition::TargetHealthBelow(percentage) => {
            target.map_or(false, |target| {
                u32::from(target.health) * 100
                    < u32::from(target.max_health)
                        * u32::from(*percentage)
            })
        }

        SpellCondition::TargetIsStunned => {
            target.map_or(false, |target| target.is_stunned)
        }

        SpellCondition::Any(conditions) => {
            conditions.iter().any(|condition| {
                condition_met(condition, caster, target)
            })
        }

        SpellCondition::All(conditions) => {
            conditions.iter().all(|condition| {
                condition_met(condition, caster, target)
            })
        }
    }
}