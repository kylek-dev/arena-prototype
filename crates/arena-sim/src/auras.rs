use crate::spells::{DamageCategory, SpellEffect, SpellId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuraError {
    NotDamageOverTime,
    UnsupportedEffect,
    InvalidTiming,
    InvalidPercentage,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DotTick {
    pub caster_id: usize,
    pub spell_id: SpellId,
    pub effect_index: usize,
    pub damage: u16,
    pub categories: Vec<DamageCategory>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ActiveAura {
    caster_id: usize,
    spell_id: SpellId,
    effect_index: usize,
    ticks_left: u32,
    kind: AuraKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum AuraKind {
    DamageOverTime {
        categories: Vec<DamageCategory>,
        damage_left: u16,
        ticks_until_hit: u32,
        interval_ticks: u32,
    },
    IncreaseDamageDone(u16),
    ReduceDamageTaken(u8),
}

// Each fighter keeps their own aura state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuraState {
    auras: Vec<ActiveAura>,
}

impl AuraState {
    pub fn apply_dot(
        &mut self,
        caster_id: usize,
        spell_id: SpellId,
        effect_index: usize,
        effect: &SpellEffect,
    ) -> Result<(), AuraError> {
        if !matches!(effect, SpellEffect::DamageOverTime { .. }) {
            return Err(AuraError::NotDamageOverTime);
        }

        self.apply_aura(caster_id, spell_id, effect_index, effect)
    }

    pub fn apply_aura(
        &mut self,
        caster_id: usize,
        spell_id: SpellId,
        effect_index: usize,
        effect: &SpellEffect,
    ) -> Result<(), AuraError> {
        let (ticks_left, kind) = match effect {
            SpellEffect::DamageOverTime {
                total_damage,
                duration_ticks,
                tick_interval_ticks,
                categories,
            } => {
                if *tick_interval_ticks == 0 || !duration_ticks.is_multiple_of(*tick_interval_ticks)
                {
                    return Err(AuraError::InvalidTiming);
                }

                (
                    *duration_ticks,
                    AuraKind::DamageOverTime {
                        categories: categories.clone(),
                        damage_left: *total_damage,
                        ticks_until_hit: *tick_interval_ticks,
                        interval_ticks: *tick_interval_ticks,
                    },
                )
            }
            SpellEffect::IncreaseDamageDone {
                percentage,
                duration_ticks,
            } => (*duration_ticks, AuraKind::IncreaseDamageDone(*percentage)),
            SpellEffect::ReduceDamageTaken {
                percentage,
                duration_ticks,
            } => {
                if *percentage > 100 {
                    return Err(AuraError::InvalidPercentage);
                }

                (*duration_ticks, AuraKind::ReduceDamageTaken(*percentage))
            }
            _ => return Err(AuraError::UnsupportedEffect),
        };

        if ticks_left == 0 {
            return Err(AuraError::InvalidTiming);
        }

        let new_aura = ActiveAura {
            caster_id,
            spell_id,
            effect_index,
            ticks_left,
            kind,
        };

        // Refresh this caster's copy of the same spell effect.
        for aura in &mut self.auras {
            if aura.caster_id == caster_id
                && aura.spell_id == spell_id
                && aura.effect_index == effect_index
            {
                *aura = new_aura;
                return Ok(());
            }
        }

        self.auras.push(new_aura);
        Ok(())
    }

    // Call once per simulation tick before applying newly landed spells.
    pub fn tick(&mut self) -> Vec<DotTick> {
        let mut hits = Vec::new();

        for aura in &mut self.auras {
            aura.ticks_left -= 1;

            let AuraKind::DamageOverTime {
                categories,
                damage_left,
                ticks_until_hit,
                interval_ticks,
            } = &mut aura.kind
            else {
                continue;
            };

            *ticks_until_hit -= 1;
            if *ticks_until_hit != 0 {
                continue;
            }

            // Include the hit due now. Round up to preserve the total damage.
            let hits_left = 1 + aura.ticks_left / *interval_ticks;
            let damage = u32::from(*damage_left).div_ceil(hits_left) as u16;
            *damage_left -= damage;
            *ticks_until_hit = *interval_ticks;

            hits.push(DotTick {
                caster_id: aura.caster_id,
                spell_id: aura.spell_id,
                effect_index: aura.effect_index,
                damage,
                categories: categories.clone(),
            });
        }

        // Emit the final DoT hit before removing expired auras.
        self.auras.retain(|aura| aura.ticks_left > 0);
        hits
    }

    pub fn damage_done_bonus(&self) -> u16 {
        let mut bonus = 0;

        for aura in &self.auras {
            if let AuraKind::IncreaseDamageDone(percentage) = aura.kind {
                bonus = bonus.max(percentage);
            }
        }

        bonus
    }

    pub fn damage_taken_reduction(&self) -> u8 {
        let mut reduction = 0;

        for aura in &self.auras {
            if let AuraKind::ReduceDamageTaken(percentage) = aura.kind {
                reduction = reduction.max(percentage);
            }
        }

        reduction
    }

    // Feed these IDs into casting conditions like TargetHasEffect.
    pub fn active_effects(&self) -> Vec<SpellId> {
        let mut spell_ids = Vec::new();

        for aura in &self.auras {
            if !spell_ids.contains(&aura.spell_id) {
                spell_ids.push(aura.spell_id);
            }
        }

        spell_ids
    }
}