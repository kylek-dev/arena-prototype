use crate::spells::{Spell, SpellEffect};


// Returns the amount of HP actually removed.
pub fn apply_damage(health: &mut u16, amount: u16) -> u16 {
    if amount >= *health {
        let actual_damage = *health;
        *health = 0;
        return actual_damage;
    }

    *health -= amount;
    return amount;
}

// Returns the amount of HP actually restored.
pub fn apply_heal(health: &mut u16, max_health: u16, amount: u16) -> u16 {
    // Normal healing cannot revive a dead unit.
    if *health == 0 || *health >= max_health {
        return 0;
    }

    let missing_health = max_health - *health;

    if amount >= missing_health {
        *health = max_health;
        return missing_health;
    }

    *health += amount;
    return amount;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectError {
    UnsupportedEffect,
}

pub fn apply_spell_effects(
    spell: &Spell,
    health: &mut u16,
    max_health: u16,
) -> Result<(), EffectError> {
    let mut updated_health = *health;

    for effect in &spell.effects {
        match effect {
            SpellEffect::Damage { amount, .. } => {
                apply_damage(&mut updated_health, *amount);
            }

            SpellEffect::Heal { amount } => {
                apply_heal(&mut updated_health, max_health, *amount);
            }

            _ => {
                return Err(EffectError::UnsupportedEffect);
            }
        }
    }

    *health = updated_health;
    return Ok(());
}


pub fn gain_resource(
    current: &mut u16,
    maximum: u16,
    amount: u16,
) -> u16