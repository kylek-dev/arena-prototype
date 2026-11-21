use std::collections::BTreeMap;

use crate::classes::{Class, Resource};
use crate::combat_state::{CastStart, CastStateError, CombatState};
use crate::spells::{Spell, SpellCondition, SpellId, SpellTarget, SpellType};

// Use your arena's stable unit ID type here.
pub type UnitId = usize;

// Borrow the player's actual values, so paying costs updates the player.
pub struct CasterContext<'a> {
    pub id: UnitId,
    pub class: Class,
    pub health: &'a mut u16,
    pub max_health: u16,
    pub resources: &'a mut BTreeMap<Resource, u16>,
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
#[derive(Clone, Debug, PartialEq, Eq)]
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

    pub fn from_pending(pending: Option<PreparedCast>) -> Self {
        Self { pending }
    }

    pub fn pending_cast(&self) -> Option<&PreparedCast> {
        self.pending.as_ref()
    }

    pub fn pending_target_id(&self) -> Option<UnitId> {
        self.pending.as_ref().and_then(|cast| cast.target_id)
    }

    /// Read-only pre-flight check matching [`Self::try_cast`] without mutating
    /// timers, health, or resources.
    pub fn can_cast(
        &self,
        combat: &CombatState,
        caster: &CasterContext<'_>,
        spell: &Spell,
        target: Option<&TargetContext<'_>>,
    ) -> Result<Option<UnitId>, CastError> {
        let (target_id, _) = self.validate_attempt(combat, caster, spell, target)?;
        Ok(target_id)
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
        let (target_id, costs) = self.validate_attempt(combat, caster, spell, target)?;

        let prepared = PreparedCast {
            spell: spell.clone(),
            caster_id: caster.id,
            target_id,
        };

        let started = combat.start_cast(spell).map_err(CastError::Timing)?;

        // Pay once, at cast start.
        // Interruptions do not refund these costs.
        *caster.health -= spell.health_cost;

        for (resource, amount) in costs {
            if let Some(current) = caster.resources.get_mut(&resource) {
                *current = current.saturating_sub(amount as u16);
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

    fn validate_attempt(
        &self,
        combat: &CombatState,
        caster: &CasterContext<'_>,
        spell: &Spell,
        target: Option<&TargetContext<'_>>,
    ) -> Result<(Option<UnitId>, BTreeMap<Resource, u32>), CastError> {
        if spell.spell_type == SpellType::Passive {
            return Err(CastError::Timing(CastStateError::PassiveSpell));
        }

        if *caster.health == 0 {
            return Err(CastError::CasterDead);
        }

        if caster.max_health == 0 {
            return Err(CastError::InvalidHealth);
        }

        if spell.class != caster.class {
            return Err(CastError::WrongClass);
        }

        if !caster.known_spells.contains(&spell.id) {
            return Err(CastError::UnknownSpell);
        }

        let resolved = resolve_target(spell, caster, target)?;

        if self.pending.is_some() || combat.is_casting() {
            return Err(CastError::Timing(CastStateError::AlreadyCasting));
        }

        check_caster_can_act(spell, caster)?;

        let remaining_ticks = combat.cooldown_remaining(spell.id);
        if !cooldown_is_ready(remaining_ticks) {
            return Err(CastError::Timing(CastStateError::SpellCooldown {
                remaining_ticks,
            }));
        }

        combat.can_cast(spell).map_err(CastError::Timing)?;

        check_target_geometry(spell, caster.id, resolved.as_ref())?;

        let conditions_met = spell
            .conditions
            .iter()
            .all(|condition| condition_met(condition, caster, resolved.as_ref()));

        if !conditions_met {
            return Err(CastError::ConditionNotMet);
        }

        // Combine repeated costs for the same resource.
        let mut costs = BTreeMap::<Resource, u32>::new();

        for cost in &spell.resource_costs {
            let total = costs.entry(cost.resource).or_insert(0);

            *total = total
                .checked_add(u32::from(cost.amount))
                .ok_or(CastError::InvalidResourceCost)?;
        }

        for (resource, amount) in &costs {
            let available = caster.resources.get(resource).copied().unwrap_or(0);

            if u32::from(available) < *amount {
                return Err(CastError::NotEnoughResource(*resource));
            }
        }

        // HP costs must leave at least 1 HP.
        if spell.health_cost >= *caster.health && spell.health_cost > 0 {
            return Err(CastError::NotEnoughHealth);
        }

        Ok((resolved.map(|target| target.id), costs))
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
            if validation.is_ok() && active.is_some_and(|cast| cast.remaining_ticks <= 1) {
                validation = validate_cast(&pending.spell, caster, target).and_then(|target_id| {
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
                let finished = self.pending.take().ok_or(CastError::StateMismatch)?;

                if finished.spell.id != id {
                    return Err(CastError::StateMismatch);
                }

                Ok(Some(finished))
            }

            None => Ok(None),
        }
    }

    // Route interrupts through here to keep both states synchronized.
    pub fn interrupt_cast(&mut self, combat: &mut CombatState) -> Option<SpellId> {
        self.pending = None;
        combat.interrupt_cast()
    }

    pub fn reset(&mut self, combat: &mut CombatState) {
        self.pending = None;
        combat.reset();
    }
}

fn check_caster(spell: &Spell, caster: &CasterContext<'_>) -> Result<(), CastError> {
    if *caster.health == 0 {
        return Err(CastError::CasterDead);
    }

    if caster.max_health == 0 {
        return Err(CastError::InvalidHealth);
    }

    check_caster_can_act(spell, caster)
}

fn check_caster_can_act(spell: &Spell, caster: &CasterContext<'_>) -> Result<(), CastError> {
    if caster.is_stunned {
        return Err(CastError::Stunned);
    }

    if caster.silence_blocks_spell {
        return Err(CastError::Silenced);
    }

    if caster.is_moving && spell.cast_ticks > 0 && !spell.can_cast_while_moving {
        return Err(CastError::Moving);
    }

    Ok(())
}

pub fn cooldown_is_ready(remaining_ticks: u32) -> bool {
    remaining_ticks == 0
}

fn resolve_target<'a>(
    spell: &Spell,
    caster: &'a CasterContext<'_>,
    target: Option<&'a TargetContext<'a>>,
) -> Result<Option<TargetContext<'a>>, CastError> {
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
    }

    Ok(resolved)
}

fn check_target_geometry(
    spell: &Spell,
    caster_id: UnitId,
    resolved: Option<&TargetContext<'_>>,
) -> Result<(), CastError> {
    if let Some(target) = resolved
        && target.id != caster_id
    {
        let range = u64::from(spell.range);

        if target.distance_squared > range * range {
            return Err(CastError::OutOfRange);
        }

        if !target.has_line_of_sight {
            return Err(CastError::NoLineOfSight);
        }
    }

    Ok(())
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

    let resolved = resolve_target(spell, caster, target)?;
    check_target_geometry(spell, caster.id, resolved.as_ref())?;

    let conditions_met = spell
        .conditions
        .iter()
        .all(|condition| condition_met(condition, caster, resolved.as_ref()));

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
            target.is_some_and(|target| target.active_effects.contains(id))
        }

        SpellCondition::TargetDoesNotHaveEffect(id) => {
            target.is_some_and(|target| !target.active_effects.contains(id))
        }

        SpellCondition::CasterHasEffect(id) => caster.active_effects.contains(id),

        // Strict comparisons without rounding the health percentage.
        SpellCondition::TargetHealthAbove(percentage) => target.is_some_and(|target| {
            u32::from(target.health) * 100 > u32::from(target.max_health) * u32::from(*percentage)
        }),

        SpellCondition::TargetHealthBelow(percentage) => target.is_some_and(|target| {
            u32::from(target.health) * 100 < u32::from(target.max_health) * u32::from(*percentage)
        }),

        SpellCondition::TargetIsStunned => target.is_some_and(|target| target.is_stunned),

        SpellCondition::Any(conditions) => conditions
            .iter()
            .any(|condition| condition_met(condition, caster, target)),

        SpellCondition::All(conditions) => conditions
            .iter()
            .all(|condition| condition_met(condition, caster, target)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spells::{ARENA_CLASS, ARENA_RESOURCE, RIFT_BOLT_CAST_TICKS, ResourceAmount};

    fn enemy_target(distance: u64, los: bool) -> TargetContext<'static> {
        TargetContext {
            id: 1,
            is_friendly: false,
            health: 100,
            max_health: 100,
            distance_squared: distance * distance,
            has_line_of_sight: los,
            is_stunned: false,
            active_effects: &[],
        }
    }

    fn resources(mana: u16) -> BTreeMap<Resource, u16> {
        BTreeMap::from([(ARENA_RESOURCE, mana)])
    }

    #[test]
    fn timed_cast_pays_once_and_rechecks_range_and_los_at_completion() {
        let mut combat = CombatState::new();
        combat.set_haste_percentage(23);
        let mut casting = CastingState::new();
        let bolt = SpellId::RiftBolt.definition();
        let mut health = 100u16;
        let mut pool = resources(100);
        let target = enemy_target(6_000, true);

        let mut caster = CasterContext {
            id: 0,
            class: ARENA_CLASS,
            health: &mut health,
            max_health: 100,
            resources: &mut pool,
            known_spells: &SpellId::ALL,
            active_effects: &[],
            is_stunned: false,
            silence_blocks_spell: false,
            is_moving: false,
        };

        assert_eq!(
            casting.try_cast(&mut combat, &mut caster, &bolt, Some(&target)),
            Ok(None)
        );
        assert_eq!(caster.resources.get(&ARENA_RESOURCE), Some(&65));
        assert_eq!(casting.pending_target_id(), Some(1));

        // Walk out of range on the final tick: cast fails, resource is NOT refunded,
        // and Rift Bolt's personal cooldown does NOT start.
        for _ in 1..RIFT_BOLT_CAST_TICKS {
            assert_eq!(casting.tick(&mut combat, &caster, Some(&target)), Ok(None));
        }
        let far = enemy_target(12_001, true);
        assert_eq!(
            casting.tick(&mut combat, &caster, Some(&far)),
            Err(CastError::OutOfRange)
        );
        assert!(casting.pending_cast().is_none());
        assert_eq!(caster.resources.get(&ARENA_RESOURCE), Some(&65));
        assert_eq!(combat.cooldown_remaining(SpellId::RiftBolt), 0);

        // Start again and lose LoS on the completion tick.
        while combat.gcd_remaining() > 0 {
            let _ = casting.tick(&mut combat, &caster, Some(&target));
        }
        assert_eq!(
            casting.try_cast(&mut combat, &mut caster, &bolt, Some(&target)),
            Ok(None)
        );
        for _ in 1..RIFT_BOLT_CAST_TICKS {
            assert_eq!(casting.tick(&mut combat, &caster, Some(&target)), Ok(None));
        }
        let blocked = enemy_target(6_000, false);
        assert_eq!(
            casting.tick(&mut combat, &caster, Some(&blocked)),
            Err(CastError::NoLineOfSight)
        );
        assert_eq!(combat.cooldown_remaining(SpellId::RiftBolt), 0);
    }

    #[test]
    fn movement_interrupts_stationary_casts_immediately_and_failed_attempts_leave_state_intact() {
        let mut combat = CombatState::new();
        let mut casting = CastingState::new();
        let bolt = SpellId::RiftBolt.definition();
        let mut health = 100u16;
        let mut pool = resources(20); // too little for Rift Bolt (35)
        let target = enemy_target(5_000, true);

        let mut caster = CasterContext {
            id: 0,
            class: ARENA_CLASS,
            health: &mut health,
            max_health: 100,
            resources: &mut pool,
            known_spells: &SpellId::ALL,
            active_effects: &[],
            is_stunned: false,
            silence_blocks_spell: false,
            is_moving: false,
        };

        assert_eq!(
            casting.try_cast(&mut combat, &mut caster, &bolt, Some(&target)),
            Err(CastError::NotEnoughResource(ARENA_RESOURCE))
        );
        assert_eq!(caster.resources.get(&ARENA_RESOURCE), Some(&20));
        assert_eq!(combat.gcd_remaining(), 0);

        *caster.resources.get_mut(&ARENA_RESOURCE).unwrap() = 100;
        caster.is_moving = true;
        assert_eq!(
            casting.try_cast(&mut combat, &mut caster, &bolt, Some(&target)),
            Err(CastError::Moving)
        );
        assert_eq!(caster.resources.get(&ARENA_RESOURCE), Some(&100));

        caster.is_moving = false;
        assert_eq!(
            casting.try_cast(&mut combat, &mut caster, &bolt, Some(&target)),
            Ok(None)
        );
        caster.is_moving = true;
        assert_eq!(
            casting.tick(&mut combat, &caster, Some(&target)),
            Err(CastError::Moving)
        );
        assert!(!combat.is_casting());
        assert_eq!(caster.resources.get(&ARENA_RESOURCE), Some(&65));
    }

    #[test]
    fn repeated_costs_health_costs_and_conditions_are_enforced_strictly() {
        let mut combat = CombatState::new();
        let mut casting = CastingState::new();
        let mut spell = Spell::active(SpellId::Pulse, "Blood Pact", ARENA_CLASS);
        spell.range = 6_000;
        spell.resource_costs = vec![
            ResourceAmount {
                resource: ARENA_RESOURCE,
                amount: 30,
            },
            ResourceAmount {
                resource: ARENA_RESOURCE,
                amount: 25,
            },
        ];
        spell.health_cost = 50;
        spell.conditions = vec![SpellCondition::All(vec![
            SpellCondition::TargetHealthBelow(50),
            SpellCondition::Any(vec![
                SpellCondition::TargetIsStunned,
                SpellCondition::CasterHasEffect(SpellId::Mend),
            ]),
        ])];

        let mut health = 50u16; // HP cost 50 would leave 0 HP -> rejected
        let mut pool = resources(54); // combined resource cost is 55
        let mut target = enemy_target(3_000, true);
        target.health = 50; // strictly below 50% requires < 50

        let mut caster = CasterContext {
            id: 0,
            class: ARENA_CLASS,
            health: &mut health,
            max_health: 100,
            resources: &mut pool,
            known_spells: &SpellId::ALL,
            active_effects: &[SpellId::Mend],
            is_stunned: false,
            silence_blocks_spell: false,
            is_moving: false,
        };

        assert_eq!(
            casting.try_cast(&mut combat, &mut caster, &spell, Some(&target)),
            Err(CastError::ConditionNotMet)
        );
        target.health = 49;
        assert_eq!(
            casting.try_cast(&mut combat, &mut caster, &spell, Some(&target)),
            Err(CastError::NotEnoughResource(ARENA_RESOURCE))
        );
        *caster.resources.get_mut(&ARENA_RESOURCE).unwrap() = 55;
        assert_eq!(
            casting.try_cast(&mut combat, &mut caster, &spell, Some(&target)),
            Err(CastError::NotEnoughHealth)
        );
        *caster.health = 51;
        let prepared = casting
            .try_cast(&mut combat, &mut caster, &spell, Some(&target))
            .unwrap()
            .unwrap();
        assert_eq!(prepared.target_id, Some(1));
        assert_eq!(*caster.health, 1);
        assert_eq!(caster.resources.get(&ARENA_RESOURCE), Some(&0));
    }

    #[test]
    fn state_mismatch_resynchronizes_combat_and_casting_states() {
        let mut combat = CombatState::new();
        let mut casting = CastingState::new();
        let bolt = SpellId::RiftBolt.definition();
        let mut health = 100u16;
        let mut pool = resources(100);
        let target = enemy_target(5_000, true);
        let caster = CasterContext {
            id: 0,
            class: ARENA_CLASS,
            health: &mut health,
            max_health: 100,
            resources: &mut pool,
            known_spells: &SpellId::ALL,
            active_effects: &[],
            is_stunned: false,
            silence_blocks_spell: false,
            is_moving: false,
        };

        // Start cast directly on combat without going through casting.
        combat.start_cast(&bolt).unwrap();
        assert_eq!(
            casting.tick(&mut combat, &caster, Some(&target)),
            Err(CastError::StateMismatch)
        );
        assert!(!combat.is_casting());
    }
}
