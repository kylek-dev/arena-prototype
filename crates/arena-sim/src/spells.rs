use crate::TICKS_PER_SECOND;
use crate::classes::{Class, Resource};
pub use crate::formulas::{BASE_GCD_MS, calculate_gcd_ticks};

/// Default arena fighter class and resource used by the shared arena kit.
pub const ARENA_CLASS: Class = Class::PlagueDoctor;
pub const ARENA_RESOURCE: Resource = Resource::Mana;
pub const MAX_RESOURCE: u16 = 100;
/// Passive resource regeneration: +1 every 3 ticks (10/s at 30 Hz).
pub const RESOURCE_REGEN_INTERVAL: u64 = 3;
pub const RESOURCE_REGEN_AMOUNT: u16 = 1;

pub const PULSE_DAMAGE: u16 = 25;
pub const PULSE_RANGE: u16 = 6_000;
pub const PULSE_COOLDOWN_TICKS: u32 = TICKS_PER_SECOND as u32;
pub const PULSE_COST: u16 = 10;

pub const RIFT_BOLT_DAMAGE: u16 = 40;
pub const RIFT_BOLT_RANGE: u16 = 12_000;
pub const RIFT_BOLT_CAST_TICKS: u32 = (TICKS_PER_SECOND as u32 * 3) / 2;
pub const RIFT_BOLT_COOLDOWN_TICKS: u32 = 2 * TICKS_PER_SECOND as u32;
pub const RIFT_BOLT_COST: u16 = 35;

pub const QUELL_RANGE: u16 = 8_000;
pub const QUELL_COOLDOWN_TICKS: u32 = 6 * TICKS_PER_SECOND as u32;
pub const QUELL_COST: u16 = 15;
pub const INTERRUPT_LOCKOUT_TICKS: u32 = 2 * TICKS_PER_SECOND as u32;

pub const MEND_HEAL: u16 = 30;
pub const MEND_RANGE: u16 = 12_000;
pub const MEND_CAST_TICKS: u32 = (TICKS_PER_SECOND as u32 * 3) / 2;
pub const MEND_COOLDOWN_TICKS: u32 = 3 * TICKS_PER_SECOND as u32;
pub const MEND_COST: u16 = 40;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SpellId {
    /// Instant ranged burst; backwards-compatible with the original Pulse attack.
    Pulse = 0,
    /// Stationary 1.5 s cast dealing heavy ranged damage.
    RiftBolt = 1,
    /// Off-GCD instant interrupt that locks out casting on a successful interrupt.
    Quell = 2,
    /// Stationary 1.5 s friendly/self heal.
    Mend = 3,
}

impl SpellId {
    pub const COUNT: usize = 4;
    pub const ALL: [Self; Self::COUNT] = [Self::Pulse, Self::RiftBolt, Self::Quell, Self::Mend];

    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Self::Pulse),
            1 => Some(Self::RiftBolt),
            2 => Some(Self::Quell),
            3 => Some(Self::Mend),
            _ => None,
        }
    }

    /// Canonical rule definition for this spell in the shared arena kit.
    pub fn definition(self) -> Spell {
        match self {
            Self::Pulse => Spell {
                id: Self::Pulse,
                name: "Pulse",
                class: ARENA_CLASS,
                spell_type: SpellType::Active,
                target: SpellTarget::Enemy,
                resource_costs: vec![ResourceAmount {
                    resource: ARENA_RESOURCE,
                    amount: PULSE_COST,
                }],
                health_cost: 0,
                cast_ticks: 0,
                cooldown_ticks: PULSE_COOLDOWN_TICKS,
                gcd: GcdType::Normal,
                range: PULSE_RANGE,
                area_radius: 0,
                conditions: Vec::new(),
                can_cast_while_moving: true,
                effects: vec![SpellEffect::Damage {
                    amount: PULSE_DAMAGE,
                    categories: vec![DamageCategory::Shadow],
                }],
            },
            Self::RiftBolt => Spell {
                id: Self::RiftBolt,
                name: "Rift Bolt",
                class: ARENA_CLASS,
                spell_type: SpellType::Active,
                target: SpellTarget::Enemy,
                resource_costs: vec![ResourceAmount {
                    resource: ARENA_RESOURCE,
                    amount: RIFT_BOLT_COST,
                }],
                health_cost: 0,
                cast_ticks: RIFT_BOLT_CAST_TICKS,
                cooldown_ticks: RIFT_BOLT_COOLDOWN_TICKS,
                gcd: GcdType::Normal,
                range: RIFT_BOLT_RANGE,
                area_radius: 0,
                conditions: Vec::new(),
                can_cast_while_moving: false,
                effects: vec![SpellEffect::Damage {
                    amount: RIFT_BOLT_DAMAGE,
                    categories: vec![DamageCategory::Shadow],
                }],
            },
            Self::Quell => Spell {
                id: Self::Quell,
                name: "Quell",
                class: ARENA_CLASS,
                spell_type: SpellType::Active,
                target: SpellTarget::Enemy,
                resource_costs: vec![ResourceAmount {
                    resource: ARENA_RESOURCE,
                    amount: QUELL_COST,
                }],
                health_cost: 0,
                cast_ticks: 0,
                cooldown_ticks: QUELL_COOLDOWN_TICKS,
                gcd: GcdType::OffGcd,
                range: QUELL_RANGE,
                area_radius: 0,
                conditions: Vec::new(),
                can_cast_while_moving: true,
                effects: vec![SpellEffect::Interrupt {
                    lockout_ticks: INTERRUPT_LOCKOUT_TICKS,
                }],
            },
            Self::Mend => Spell {
                id: Self::Mend,
                name: "Mend",
                class: ARENA_CLASS,
                spell_type: SpellType::Active,
                target: SpellTarget::Friendly,
                resource_costs: vec![ResourceAmount {
                    resource: ARENA_RESOURCE,
                    amount: MEND_COST,
                }],
                health_cost: 0,
                cast_ticks: MEND_CAST_TICKS,
                cooldown_ticks: MEND_COOLDOWN_TICKS,
                gcd: GcdType::Normal,
                range: MEND_RANGE,
                area_radius: 0,
                conditions: Vec::new(),
                can_cast_while_moving: false,
                effects: vec![SpellEffect::Heal { amount: MEND_HEAL }],
            },
        }
    }
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

    IncreaseDamageDone {
        percentage: u16,
        duration_ticks: u32,
    },

    ReduceDamageTaken {
        percentage: u8,
        duration_ticks: u32,
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

    Interrupt {
        lockout_ticks: u32,
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
        Self {
            spell_type: SpellType::Passive,
            target: SpellTarget::NoTarget,
            gcd: GcdType::OffGcd,
            effects,
            ..Self::active(id, name, class)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arena_spell_ids_round_trip_and_define_valid_kit() {
        for (idx, spell_id) in SpellId::ALL.into_iter().enumerate() {
            assert_eq!(spell_id.index(), idx);
            assert_eq!(SpellId::from_index(idx), Some(spell_id));
            let def = spell_id.definition();
            assert_eq!(def.id, spell_id);
            assert_eq!(def.class, ARENA_CLASS);
            assert_eq!(def.spell_type, SpellType::Active);
            assert!(def.range > 0);
            assert!(!def.effects.is_empty());
        }
        assert_eq!(SpellId::from_index(SpellId::COUNT), None);

        let passive = Spell::passive(
            SpellId::Pulse,
            "Haste Passive",
            ARENA_CLASS,
            vec![SpellEffect::ModifyGcd {
                multiplier_percentage: 50,
            }],
        );
        assert_eq!(passive.spell_type, SpellType::Passive);
        assert_eq!(passive.target, SpellTarget::NoTarget);
        assert_eq!(passive.gcd, GcdType::OffGcd);
    }
}
