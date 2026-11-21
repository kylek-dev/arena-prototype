#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Class {
    Necromancer,
    PlagueDoctor,
    Gladiator,
    Nightblade,
    Bloodbinder,
    Wildkeeper,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Role {
    DPS,
    Healer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

            Self::PlagueDoctor => vec![Resource::Mana, Resource::Toxicity],

            Self::Gladiator => vec![Resource::Momentum],

            Self::Nightblade => vec![Resource::Energy, Resource::Dread],

            Self::Bloodbinder => vec![Resource::Blood],

            Self::Wildkeeper => vec![Resource::LifeEssence],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_class_declares_at_least_one_unique_resource() {
        let all = [
            Class::Necromancer,
            Class::PlagueDoctor,
            Class::Gladiator,
            Class::Nightblade,
            Class::Bloodbinder,
            Class::Wildkeeper,
        ];
        for class in all {
            let resources = class.resources();
            assert!(!resources.is_empty(), "{class:?}");
            let unique: std::collections::BTreeSet<_> = resources.iter().copied().collect();
            assert_eq!(unique.len(), resources.len(), "{class:?}");
        }
        assert_eq!(
            Class::PlagueDoctor.resources(),
            vec![Resource::Mana, Resource::Toxicity]
        );
    }
}
