#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Class {
    Necromancer,
    PlagueDoctor,
    Gladiator,
    Nightblade,
    Bloodbinder,
    Wildkeeper,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    DPS,
    Healer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Resource {
    Mana,
    Energy,
    Rage,
    Toxicity,
    Momentum,
    Dread,
    Blood,
    Suffering,
    LifeEssence,
}

impl Class {
    pub fn resources(&self) -> Vec<Resource> {
        match self {

            Self::Necromancer => vec![Resource::Suffering],


            Self::PlagueDoctor => vec![
                Resource::Mana,
                Resource::Toxicity,
            ],

            Self::Gladiator => vec![Resource::Momentum],


            Self::Nightblade => vec![
                Resource::Energy,
                Resource::Dread,
            ],

            // Generator spells cost HP and build Blood.
            // Spenders consume Blood to cast powerful spells.
            Self::Bloodbinder => vec![Resource::Blood],

            // Companion attacks and basic nature spells generate Life Essence.
            Self::Wildkeeper => vec![Resource::LifeEssence],
        }
    }
}