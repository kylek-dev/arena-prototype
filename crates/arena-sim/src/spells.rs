use crate::classes::{Class, Resource};

pub const TICKS_PER_SECOND: u32 = 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SpellId {
    // Add your own spell IDs here when you define your spells.
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SpellType {
    Active,
    Passive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SpellTarget {
    Enemy,
    Friendly, // Includes the caster.
    SelfTarget,
    NoTarget, // Used by passives.
}

// Damage can have several categories, such as both Blood and Pet.
// Tag damage from owned summons as Pet, including their basic attacks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DamageCategory {
    Physical,
    Shadow,
    Poison,
    Blood,
    Nature,
    Pet,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SummonId {
    // Add your own summon IDs here when you define your summons.
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceAmount {
    pub resource: Resource,
    pub amount: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpellEffect {
    // Effects performed by active spells.
    Damage {
        amount: u16,
        categories: Vec<DamageCategory>,
    },
    Heal {
        amount: u16,
    },
    DamageOverTime {
        total_damage: u16,
        duration_ticks: u32,
        tick_interval_ticks: u32,
        categories: Vec<DamageCategory>,
    },
    Summon {
        summon: SummonId,
        count: u8,
        duration_ticks: u32,
    },
    GainResource {
        resource: Resource,
        amount: u16,
    },
    ReduceResource {
        resource: Resource,
        amount: u16,
    },

    // Modifiers supplied by unlocked passive spells.
    IncreaseSpellDamage {
        spell: SpellId,
        percentage: u16,
    },
    IncreaseCategoryDamage {
        category: DamageCategory,
        percentage: u16,
    },
    RemoveTargetHealthRestriction {
        spell: SpellId,
    },
    IncreaseSummonCount {
        spell: SpellId,
        amount: u8,
    },
    ChanceToReplaceSummon {
        spell: SpellId,
        original: SummonId,
        replacement: SummonId,
        chance_percentage: u8, // 0 through 100. Roll once per summon.
    },
    AddDamageOverTime {
        spell: SpellId,
        // Total extra damage as a percentage of the spell's direct damage,
        // after damage bonuses. Spread this total across the whole duration.
        percentage: u16,
        // Optional duration override. Combat code must define the fallback.
        duration_ticks: Option<u32>,
        tick_interval_ticks: u32,
    },
    BecomeAoE {
        spell: SpellId,
        radius: u16,
    },
    AllowCastingWhileMoving {
        spell: SpellId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spell {
    pub id: SpellId,
    pub name: &'static str,
    pub class: Class,
    pub spell_type: SpellType,
    pub target: SpellTarget,
    pub resource_costs: Vec<ResourceAmount>,
    pub health_cost: u16,
    pub cast_ticks: u32,
    pub cooldown_ticks: u32,
    pub range: u16,
    pub area_radius: u16, // zero means a single target.
    // Some(limit) requires target health to be at or below that percentage.
    pub target_health_max_percentage: Option<u8>,
    pub can_cast_while_moving: bool,
    pub effects: Vec<SpellEffect>,
}

impl Spell {
    pub fn active(id: SpellId, name: &'static str, class: Class) -> Self {
        Self {
            id,
            name,
            class,
            spell_type: SpellType::Active,
            target: SpellTarget::Enemy,
            resource_costs: Vec::new(),
            health_cost: 0,
            cast_ticks: 0,
            cooldown_ticks: 0,
            range: 0,
            area_radius: 0,
            target_health_max_percentage: None,
            can_cast_while_moving: false,
            effects: Vec::new(),
        }
    }

    pub fn passive(
        id: SpellId,
        name: &'static str,
        class: Class,
        effects: Vec<SpellEffect>,
    ) -> Self {
        let mut spell = Self::active(id, name, class);
        spell.spell_type = SpellType::Passive;
        spell.target = SpellTarget::NoTarget;
        spell.effects = effects;
        spell
    }
}