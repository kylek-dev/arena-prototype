use crate::classes::{Class, Resource};
pub use crate::formulas::{BASE_GCD_MS, calculate_gcd_ticks};

pub const TICKS_PER_SECOND: u32 = crate::TICKS_PER_SECOND as u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SpellId {
    // Add spell IDs here when creating spells.
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SummonId {
    // Add summon IDs here when creating summons.
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GcdType {
    Normal,
    OffGcd,
    Custom(u32), // Duration in ticks.
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpellCondition {
    TargetHasEffect(SpellId),
    TargetDoesNotHaveEffect(SpellId),
    CasterHasEffect(SpellId),

    TargetHealthAbove(u8),
    TargetHealthBelow(u8),

    TargetIsStunned,

    // At least one condition must be true.
    Any(Vec<SpellCondition>),

    // Every condition must be true.
    All(Vec<SpellCondition>),
}

// Damage may belong to multiple categories.
// Damage from owned summons should include Pet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DamageCategory {
    Physical,
    Shadow,
    Poison,
    Blood,
    Nature,
    Pet,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceAmount {
    pub resource: Resource,
    pub amount: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpellEffect {
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

    Slow {
        percentage: u8,
        duration_ticks: u32,
    },

    // Applies effects only when a condition is true.
    Conditional {
        condition: SpellCondition,
        effects: Vec<SpellEffect>,
    },

    // Passive spell modifications.

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
        chance_percentage: u8,
    },

    AddDamageOverTime {
        spell: SpellId,
        percentage: u16,
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

    // Hidden passives can modify a class's GCD.
    ModifyGcd {
        multiplier_percentage: u16,
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

    // Separate from the spell's personal cooldown.
    pub gcd: GcdType,

    pub range: u16,
    pub area_radius: u16, // Zero means single target.

    // All conditions must pass unless Any is used.
    pub conditions: Vec<SpellCondition>,

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

            gcd: GcdType::Normal,

            range: 0,
            area_radius: 0,

            conditions: Vec::new(),

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
        spell.gcd = GcdType::OffGcd;
        spell.effects = effects;

        spell
    }
}