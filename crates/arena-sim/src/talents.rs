pub struct Talent {
    pub name: &'static str,
    pub effects: Vec<EffectTalent>,
}

pub enum EffectTalent {
    // Unlock a new ability.
    UnlockSpell(SpellId),

    // Unlock a passive.
    UnlockPassive(PassiveId),

    // Increase a specific spell's damage.
    AddDamageOverTime {
        spell: SpellId,
        percentage: u16,
        duration: Option<u32>

    },

    // Increase damage for a category of spells.
    IncreaseCategoryDamage {
        category: SpellCategory,
        percentage: u16,
    },

    // Change how a spell works.
    ModifySpell {
        spell: SpellId,
        modification: ModificationSpell,
    },

    IncreaseDamage {
        spell: SpellId,
        percentage: u16,
    }
}

pub enum ModificationSpell {
    AddDamageOverTime,
    BecomeAoE,
    AllowCastingWhileMoving,
    IncreaseDamage,

}