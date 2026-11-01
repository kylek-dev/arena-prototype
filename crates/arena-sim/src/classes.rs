#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Classe {
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
pub enum Ressource {
    Mana,
    Energy,
    Rage,
    Blood,
    Pain,
}


impl Classe {
    pub fn ressource(&self) -> Ressource {
        match self {
            Self::Necromancer => Ressource::Mana,
            Self::PlagueDoctor => Ressource::Mana,
            Self::Gladiator => Ressource::Rage,
            Self::Nightblade => Ressource::Energy,
            Self::Bloodbinder => Ressource::Blood,
            Self::Wildkeeper => Ressource::Mana,
        }
    }
}