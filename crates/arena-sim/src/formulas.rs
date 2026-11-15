use crate::TICKS_PER_SECOND;

pub const CRIT_DAMAGE_PERCENTAGE: u32 = 150;
pub const BASE_GCD_MS: u32 = 1_150;
pub const RESILIENCE_PERCENTAGE: u8 = 30;
pub const RESILIENCE_PROC_CHANCE: u8 = 30;

// The match supplies a roll from 0 to 99.
pub fn calculate_crit_damage(damage: u16, crit_percentage: u8, roll: u8) -> u16 {
    if roll >= crit_percentage.min(100) {
        return damage;
    }

    let critical_damage = u32::from(damage) * CRIT_DAMAGE_PERCENTAGE / 100;
    critical_damage.min(u32::from(u16::MAX)) as u16
}

// Every full 5% haste removes 50 ms. Use 100 for normal GCD or 50 for half.
pub fn calculate_gcd_ticks(haste_percentage: u32, gcd_multiplier_percentage: u32) -> u32 {
    let haste_steps = haste_percentage / 5;
    let reduction_ms = haste_steps.saturating_mul(50);
    let gcd_ms = BASE_GCD_MS.saturating_sub(reduction_ms);
    let scaled_ms = u64::from(gcd_ms) * u64::from(gcd_multiplier_percentage) / 100;

    // Round up to the next simulation tick.
    (scaled_ms * TICKS_PER_SECOND).div_ceil(1_000) as u32
}

// Count successful prior applications of this CC category on this target
// during the current DR window. A result of 0 means no CC is applied.
pub fn calculate_cc_duration_ticks(base_ticks: u32, prior_applications: u8) -> u32 {
    match prior_applications {
        0 => base_ticks,
        1 => base_ticks.div_ceil(2),
        _ => 0,
    }
}

pub fn damage_after_buffs(damage: u16, damage_done_bonus: u16, damage_taken_reduction: u8) -> u16 {
    let outgoing_multiplier = 100 + u64::from(damage_done_bonus);
    let incoming_multiplier = 100 - u64::from(damage_taken_reduction.min(100));
    let adjusted_damage = u64::from(damage) * outgoing_multiplier * incoming_multiplier / 10_000;

    adjusted_damage.min(u64::from(u16::MAX)) as u16
}

// The match supplies a chance roll from 0 to 99 and a reduction roll from 1 to 30.
pub fn damage_after_resilience(damage: u16, chance_roll: u8, reduction_roll: u8) -> u16 {
    if chance_roll >= RESILIENCE_PROC_CHANCE {
        return damage;
    }

    let reduction = reduction_roll.min(RESILIENCE_PERCENTAGE);
    let prevented_damage = u32::from(damage) * u32::from(reduction) / 100;

    damage - prevented_damage as u16
}