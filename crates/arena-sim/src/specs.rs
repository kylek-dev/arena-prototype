use crate::classes::{Classe, Role};

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
    pub fn classe(&self) -> Classe {
        match self {
            Self::Summoner | Self::Lich => Classe::Necromancer,

            Self::Toxicologist
            | Self::PlagueSurgeon
            | Self::Pestilent => Classe::PlagueDoctor,

            Self::Berserker
            | Self::Duelist
            | Self::Champion => Classe::Gladiator,

            Self::Assassin | Self::DarkRanger => Classe::Nightblade,

            Self::Hemomancer | Self::BloodHealer => Classe::Bloodbinder,

            Self::Beastmaster | Self::NatureGuardian => Classe::Wildkeeper,
        }
    }

    pub fn role(&self) -> Role {
        match self {
            Self::PlagueSurgeon
            | Self::BloodHealer
            | Self::NatureGuardian => Role::Healer,

            _ => Role::DPS,
        }
    }
}