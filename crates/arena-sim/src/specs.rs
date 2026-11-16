use crate::classes::{Class, Role};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Specialisation {
    // Necromancer
    Summoner,
    Lich,

    // Plague Doctor
    Toxicologist,
    PlagueSurgeon,
    Pestilent,

    // Gladiator
    Berserker,
    Duelist,
    Champion,

    // Nightblade
    Assassin,
    DarkRanger,

    // Bloodbinder
    Hemomancer,
    BloodHealer,

    // Wildkeeper
    Beastmaster,
    NatureGuardian,
}

impl Specialisation {
    pub fn classe(&self) -> Class {
        match self {
            Self::Summoner | Self::Lich => Class::Necromancer,

            Self::Toxicologist | Self::PlagueSurgeon | Self::Pestilent => Class::PlagueDoctor,

            Self::Berserker | Self::Duelist | Self::Champion => Class::Gladiator,

            Self::Assassin | Self::DarkRanger => Class::Nightblade,

            Self::Hemomancer | Self::BloodHealer => Class::Bloodbinder,

            Self::Beastmaster | Self::NatureGuardian => Class::Wildkeeper,
        }
    }

    pub fn role(&self) -> Role {
        match self {
            Self::PlagueSurgeon | Self::BloodHealer | Self::NatureGuardian => Role::Healer,

            _ => Role::DPS,
        }
    }
}
