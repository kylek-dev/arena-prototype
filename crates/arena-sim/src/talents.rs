use crate::classes::Class;
use crate::spells::SpellId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TalentId {
    RiftMastery,
    QuickQuell,
    DeepMend,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Talent {
    pub id: TalentId,
    pub name: &'static str,
    pub class: Class,
    pub required_talents: Vec<TalentId>,
    pub effects: Vec<EffectTalent>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectTalent {
    UnlockSpell(SpellId),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spells::ARENA_CLASS;

    #[test]
    fn talents_can_reference_prerequisites_and_unlock_spells() {
        let talent = Talent {
            id: TalentId::RiftMastery,
            name: "Rift Mastery",
            class: ARENA_CLASS,
            required_talents: vec![TalentId::QuickQuell],
            effects: vec![EffectTalent::UnlockSpell(SpellId::RiftBolt)],
        };
        assert_eq!(talent.id, TalentId::RiftMastery);
        assert_eq!(talent.required_talents, vec![TalentId::QuickQuell]);
        assert_eq!(
            talent.effects,
            vec![EffectTalent::UnlockSpell(SpellId::RiftBolt)]
        );
    }
}
