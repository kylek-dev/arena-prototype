use crate::classes::Class;
use crate::spells::SpellId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TalentId {




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