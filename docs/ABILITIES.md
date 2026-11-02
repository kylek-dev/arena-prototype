
# Ability Data Contract (Draft v0.2)

**Owner:** Skide (Gameplay)  
**Status:** Proposal — requires team review  
**Format:** TOML, one `[[abilities]]` entry per ability  
**Scope:** Static ability definitions, not runtime combat state

## 1. Goals

- Designers can create and balance abilities without writing ability-specific Rust functions.
- The server/shared simulation handles ability validation and combat outcomes.
- The client handles animations, VFX, icons, cast bars, and combat logs.
- Ability IDs must be unique and remain stable.
- Ability data is loaded and validated before a match starts.
- Client and server use the same ability definitions, but the server remains authoritative.

## 2. File Layout

Proposed location: `assets/data/abilities.toml`

The file contains one `[[abilities]]` entry per ability.

See `abilities.example.toml` for five proposed test abilities:

- Soul Rot (Necromancer)
- Grim Harvest (Necromancer)
- Toxic Dart (PlagueDoctor)
- Decapitate (Gladiator)
- Renewal (Wildkeeper)

These abilities are examples for testing the data format. Their damage, costs, cooldowns, and other values may change during balancing.

No runtime loader is assumed to exist yet.

## 3. Ability Fields

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | string | Yes | Unique ability identifier |
| `class` | string | Yes | Class that owns the ability |
| `spec` | string | No | Specialization restriction, if applicable |
| `name` | string | Yes | Display name |
| `cast_time_ms` | integer | Yes | Casting time in milliseconds; 0 means instant |
| `cooldown_ms` | integer | Yes | Ability cooldown |
| `gcd_ms` | integer | Yes | Global cooldown; 0 means off-GCD |
| `resource` | string | Yes | Resource required to cast |
| `resource_cost` | integer | Yes | Amount of resource consumed |
| `range` | number | Yes | Maximum range in game-world units |
| `needs_los` | boolean | Yes | Whether line of sight is required |
| `target_type` | string | Yes | Enemy, Ally, Self, Ground, or Cone |
| `cone_angle` | number | Conditional | Cone angle in degrees, required for Cone targeting |
| `school` | string | Yes | Spell school used for interrupt lockouts |
| `animation_clip` | string | Yes | Animation identifier |
| `vfx_id` | string | Yes | Visual effect identifier |
| `icon_id` | string | Yes | Ability icon identifier |
| `condition` | table | No | Additional casting requirements |
| `effects` | array of tables | Yes | Effects applied when the ability succeeds |

All times are expressed in milliseconds.

Damage, healing, and resource values are fixed prototype numbers. Scaling formulas may be added later.

### 3.1 Target Types

- `Enemy`: Targets an enemy unit.
- `Ally`: Targets a friendly unit.
- `Self`: Targets the caster.
- `Ground`: Targets a position in the world.
- `Cone`: Affects valid targets inside a cone originating from the caster.

For Cone abilities, `range` represents the cone's maximum reach, and `cone_angle` represents its total width in degrees.

The client supplies an aiming direction, but the server determines which targets are actually hit.

## 4. Effect Types

Each ability contains one or more `[[abilities.effects]]` entries.

Effects are processed in their listed order.

### 4.1 Basic Effects

| Type | Fields | Description |
|---|---|---|
| `Damage` | `amount` | Deals damage to a target |
| `Heal` | `amount` | Restores health up to maximum HP |
| `Absorb` | `amount`, `duration_ms` | Applies a temporary damage shield |
| `ApplyCC` | `category`, `duration_ms` | Applies crowd control, subject to DR and immunity |

### 4.2 Damage Over Time

**Type:** `DamageOverTime`

Fields:

- `amount`: Damage dealt per tick.
- `duration_ms`: Total effect duration.
- `tick_interval_ms`: Time between damage ticks.

Example:

```toml
[[abilities.effects]]
type = "DamageOverTime"
amount = 12
duration_ms = 12000
tick_interval_ms = 2000
```

This example deals 12 damage every 2 seconds for 12 seconds.

Assuming the first tick occurs after 2 seconds, it deals 72 total damage over 6 ticks.

DoT timing and stacking rules must be implemented in the simulation.

### 4.3 Resource Generation

**Type:** `GenerateResource`

Fields:

- `resource`: Resource being generated.
- `amount`: Amount generated per trigger.
- `trigger`: Event that causes resource generation.

Example:

```toml
[[abilities.effects]]
type = "GenerateResource"
resource = "Souls"
amount = 1
trigger = "OnDotTick"
```

For v0.2, `OnDotTick` refers to the preceding `DamageOverTime` effect in the same ability.

Soul Rot uses this mechanic to generate Souls whenever its damage-over-time effect ticks.

The simulation must prevent resource generation beyond the resource's maximum capacity.

Other triggers may be added later.

### 4.4 Summoning

**Type:** `Summon`

Fields:

- `unit`: Type of unit to summon.
- `count`: Number of units summoned per successful trigger.
- `per_target_hit`: Whether summoning occurs for each valid target hit.
- `resource`: Additional resource consumed by the summon effect.
- `resource_cost`: Resource cost per summoned unit.
- `max_summons`: Maximum number of units created by one activation.

Example:

```toml
[[abilities.effects]]
type = "Summon"
unit = "Ghoul"
count = 1
per_target_hit = true
resource = "Souls"
resource_cost = 1
max_summons = 3
```

Grim Harvest uses this effect.

It summons one Ghoul per valid enemy hit, consuming one Soul per Ghoul, up to three Ghouls per cast.

If there are insufficient Souls, only the number of Ghouls that can be paid for are summoned.

The ability's main resource cost is separate from the summon effect's resource cost.

Summoned units are controlled and validated by the authoritative simulation.

Summon duration, pet AI, and maximum active pet count are not yet defined.

## 5. Conditional Casting

Some abilities have additional requirements before they can be activated.

These requirements are stored in an optional `[abilities.condition]` table.

### 5.1 Target Health Percentage

**Type:** `TargetHealthPercentage`

Fields:

- `above`: Target health percentage threshold.
- `below`: Target health percentage threshold.
- `operator`: How the thresholds are combined.

Example:

```toml
[abilities.condition]
type = "TargetHealthPercentage"
above = 80
below = 20
operator = "OR"
```

Decapitate uses this condition.

It can only be activated when the enemy has:

- More than 80% HP, OR
- Less than 20% HP.

The thresholds are exclusive. Exactly 80% or 20% HP does not satisfy the condition.

The server checks the target's current health percentage before executing the ability.

Additional condition types may be added later.

## 6. Resources

Each class can use one or more resources.

Proposed resource identifiers:

| Resource | Intended Use |
|---|---|
| `Mana` | Spellcasting and healing |
| `Energy` | Fast-paced abilities |
| `Rage` | Warrior-style combat |
| `Pain` | Necromancer abilities |
| `Souls` | Secondary Necromancer resource |
| `Momentum` | Gladiator abilities |
| `Blood` | Bloodbinder abilities |
| `LifeEssence` | Wildkeeper abilities |

These identifiers are provisional and must match the class/resource registry.

### 6.1 Primary and Secondary Resources

An ability has one primary resource cost using:

- `resource`
- `resource_cost`

Certain effects may generate or consume additional resources.

Example:

- Soul Rot costs Pain and generates Souls.
- Grim Harvest costs Pain and consumes Souls to summon Ghouls.
- Decapitate costs Momentum.

Each resource must have a maximum capacity defined by the gameplay system.

Resource regeneration, decay, and starting values are outside the scope of this document.

## 7. Casting and Validation

Proposed ability execution flow:

1. The client sends an ability ID and target reference or aiming information.
2. The simulation checks that the player knows the ability and can use it with their class/spec.
3. Check whether the caster is alive and allowed to act.
4. Check active casts, GCD, individual cooldown, and available primary resources.
5. Check target type, target existence, range, and line of sight.
6. Check any additional casting conditions.
7. If the ability has a cast time, begin casting and revalidate relevant conditions on completion.
8. On successful activation, spend the primary resource and start the cooldown/GCD.
9. Resolve the ability's effects in their listed order, including any secondary resource costs.
10. Emit combat events for animations, VFX, UI, and combat logs.

Rejected ability requests must not consume resources.

The client never decides authoritative damage, healing, CC duration, or resource changes.

### 7.1 Open Casting Decisions

The team still needs to decide:

- Whether cooldowns and GCD start at cast start or completion.
- When resources are spent and whether interrupted casts receive refunds.
- Whether range and LoS are rechecked at cast completion.
- How ground-target positions are represented.
- Whether DoTs refresh, stack, or overwrite existing applications.
- How temporary shields stack.
- Whether failed secondary effects affect the rest of an ability.
- How interrupted casts interact with cooldowns.

## 8. Crowd Control and Diminishing Returns

Initial CC categories:

- `Stun`
- `Incapacitate`
- `DisorientFear`
- `Silence`
- `Root`
- `InterruptLockout`

### 8.1 Proposed DR Behavior

Diminishing returns are tracked per target and CC category.

Repeated applications of the same category within its reset window receive progressively shorter durations, eventually resulting in immunity.

After the reset window expires, the category returns to its initial state.

Different CC categories maintain independent DR states.

### 8.2 Open DR Decisions

The team must determine:

- Duration reduction percentages.
- DR reset window.
- Whether the window begins on application or effect expiration.
- Whether immune or failed applications advance DR.
- How simultaneous CC applications are ordered.
- Whether InterruptLockout uses DR or a separate mechanic.

Exact values will be determined through playtesting rather than copied directly from another game.

## 9. Data Validation

A future Rust loader should use Serde to deserialize ability definitions.

Unknown fields and unsupported effect types should be rejected.

Validation should fail if:

- TOML cannot be parsed.
- An ability ID is empty or duplicated.
- A class, specialization, resource, school, or target type is unknown.
- An ability is assigned to an invalid class/spec combination.
- Cast times, cooldowns, costs, ranges, or effect values are negative.
- Required effect fields are missing.
- An effect requiring a duration has a zero or invalid duration.
- `resource = "None"` is used with a nonzero cost.
- A CC category is unknown.
- A DoT has an invalid tick interval or duration.
- A `GenerateResource` effect references an unsupported trigger or has no matching DoT.
- A Cone ability has no valid `cone_angle`.
- A condition uses an unsupported type or invalid thresholds.
- A summoned unit does not exist in the unit registry.
- `max_summons` or summon count is zero.
- Referenced animation, VFX, or icon IDs are invalid once asset registries exist.

For the initial prototype, DoT durations should be divisible by their tick intervals to avoid ambiguous partial ticks.

The loader and validation tests are not implemented yet.

## 10. Example Ability Interactions

### 10.1 Necromancer — Soul Rot

- Applies a Shadow damage-over-time effect.
- Deals damage every 2 seconds for 12 seconds.
- Generates one Soul per tick.
- Souls can be spent on other abilities.

### 10.2 Necromancer — Grim Harvest

- Sweeps a spectral scythe in a cone.
- Damages valid enemies hit by the cone.
- Consumes Souls to summon Ghouls.
- Summons one Ghoul per enemy hit, up to three per activation.

### 10.3 Gladiator — Decapitate

- Melee Physical ability.
- Costs Momentum.
- Only usable against enemies above 80% HP or below 20% HP.
- Deals direct damage.

### 10.4 PlagueDoctor — Toxic Dart

- Instant Nature damage ability.
- Costs Mana.
- Deals direct damage to an enemy.

### 10.5 Wildkeeper — Renewal

- Casted Nature healing ability.
- Costs Mana.
- Restores health to an ally.

All numerical values are provisional.

## 11. Ownership

**Gameplay — Skide**

- Ability data format.
- Class and spell design.
- Resource mechanics.
- Spell tuning.
- CC and diminishing returns rules.
- Casting and gameplay feedback requirements.

**Simulation / Backend**

- Authoritative casting validation.
- Cooldowns and resource state.
- Damage, healing, and effect resolution.
- Summoned unit state.
- Deterministic combat events.

**Client / Art**

- Animation playback.
- VFX and icon lookup.
- Cast bars.
- Combat log display.
- Ability feedback and visual presentation.

The client must not decide authoritative combat outcomes.

## 12. Future Extensions

Included in proposed v0.2:

- Direct damage and healing.
- Absorbs and crowd control.
- Damage over time.
- Resource generation.
- Summoned units.
- Cone targeting.
- Conditional casting.

Not yet included:

- Ability charges.
- Healing over time.
- Dispels and cleanses.
- Talent-driven spell modifications.
- Projectile travel time.
- Damage and healing scaling formulas.
- Pet AI and pet commands.
- Spell proc chances.
- Multiple simultaneous casting conditions.

These should be introduced as explicit data fields or effect variants when required.

## 13. Implementation Status

This document describes the proposed ability data contract.

The example TOML definitions are not yet connected to the Rust simulation.

Before implementation, the team should review:

1. Ability field names and supported effect types.
2. Resource identifiers and class/spec restrictions.
3. Cone targeting and conditional casting.
4. DoT and summon behavior.
5. Casting, cooldown, and DR rules.

Once approved, the next step is implementing the Rust data structures, TOML loader, and validation tests.
