use crate::specs::Specialisation;

pub const RESILIENCE_PERCENTAGE: u8 = 30;
pub const SECONDARY_STAT_BUDGET: u8 = 100 - RESILIENCE_PERCENTAGE;
pub const RESILIENCE_PROC_CHANCE: u8 = 30;

// Every player of the same spec gets the same template.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatTemplate {
    crit: u8,
    mastery: u8,
    haste: u8,
}

impl StatTemplate {
    pub const fn new(crit: u8, mastery: u8, haste: u8) -> Self {
        let total = crit as u16 + mastery as u16 + haste as u16;

        assert!(
            total == SECONDARY_STAT_BUDGET as u16,
            "Stats must add up to 70"
        );

        Self {
            crit,
            mastery,
            haste,
        }
    }

    pub const fn crit(self) -> u8 {
        self.crit
    }

    pub const fn mastery(self) -> u8 {
        self.mastery
    }

    pub const fn haste(self) -> u8 {
        self.haste
    }

    pub const fn resilience(self) -> u8 {
        RESILIENCE_PERCENTAGE
    }
}

// The arguments are crit, mastery, and haste, in that order.
const STARTER_TEMPLATE: StatTemplate = StatTemplate::new(24, 23, 23);

impl Specialisation {
    pub const fn stat_template(self) -> StatTemplate {
        match self {
            Self::Summoner => STARTER_TEMPLATE,
            Self::Lich => STARTER_TEMPLATE,
            Self::Toxicologist => STARTER_TEMPLATE,
            Self::PlagueSurgeon => STARTER_TEMPLATE,
            Self::Pestilent => STARTER_TEMPLATE,
            Self::Berserker => STARTER_TEMPLATE,
            Self::Duelist => STARTER_TEMPLATE,
            Self::Champion => STARTER_TEMPLATE,
            Self::Assassin => STARTER_TEMPLATE,
            Self::DarkRanger => STARTER_TEMPLATE,
            Self::Hemomancer => STARTER_TEMPLATE,
            Self::BloodHealer => STARTER_TEMPLATE,
            Self::Beastmaster => STARTER_TEMPLATE,
            Self::NatureGuardian => STARTER_TEMPLATE,
        }
    }
}

// The match rolls 0 to 99 for the chance and 1 to 30 for the reduction.
// Call this before subtracting HP.
pub fn damage_after_resilience(damage: u16, chance_roll: u8, reduction_roll: u8) -> u16 {
    if chance_roll >= RESILIENCE_PROC_CHANCE {
        return damage;
    }

    let reduction = reduction_roll.min(RESILIENCE_PERCENTAGE);
    let prevented_damage = u32::from(damage) * u32::from(reduction) / 100;

    damage - prevented_damage as u16
}