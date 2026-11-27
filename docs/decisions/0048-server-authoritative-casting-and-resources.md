# ADR 0048: Server-authoritative casting, cooldowns, resources and interrupts

Date: 2026-11-21. **Status: proposed (Lane A rules, protocol, server, bot and bridge implemented; Lane B HUD visuals queued).**

Builds on [ADR 0044](0044-authoritative-match-server.md) and addresses
[`planning/issues/server-casting-resources.md`](../../planning/issues/server-casting-resources.md).

## Context

ADR 0044 established server-authoritative movement, jumping, facing and single-button
Pulse attacks via per-actor input slots and phased simultaneous resolution, while
explicitly deferring cast times, global cooldown (GCD), per-spell cooldowns,
resources and interrupts until after the network duel worked.

`crates/arena-sim/src/casting.rs`, `combat_state.rs` and `talents.rs` existed in
the repository without being declared in `lib.rs` (and `spells.rs` carried a
duplicate `TICKS_PER_SECOND` constant and an empty `SpellId` enum).

## Decision

### 1. Canonical combat modules in `arena-sim`

- Declare `casting`, `combat_state` and `talents` in `crates/arena-sim/src/lib.rs`,
  remove the duplicate `TICKS_PER_SECOND` from `spells.rs`, and replace
  `HashMap` with `BTreeMap` in `combat_state.rs` and `casting.rs` so timer and
  resource iteration is strictly deterministic.
- Define a minimal four-spell shared arena kit in `spells.rs` (`SpellId` 0–3)
  to exercise every mechanic end to end:
  - **`Pulse`** (0): instant enemy damage (25 dmg, 6 m, 30-tick cooldown, normal GCD, 10 resource).
  - **`RiftBolt`** (1): stationary 45-tick (1.5 s) enemy damage cast (40 dmg, 12 m, 60-tick personal cooldown started on completion, normal GCD, 35 resource).
  - **`Quell`** (2): off-GCD instant enemy interrupt (8 m, 180-tick cooldown, 15 resource; interrupts an active cast and applies a 60-tick cast lockout when it lands on an active cast, or whiffs onto cooldown if the target is not casting).
  - **`Mend`** (3): stationary 45-tick (1.5 s) friendly/self heal (30 heal capped at `MAX_HEALTH`, 12 m, 90-tick personal cooldown on completion, normal GCD, 40 resource).
- Every fighter starts with `MAX_RESOURCE = 100` primary resource (`Resource::Mana`)
  and regenerates `+1` every 3 ticks (`10/s` at 30 Hz) while alive, derived solely
  from `Arena::tick` with no hidden accumulator. Normal GCD uses the starter stat
  template's 23% haste via `formulas::calculate_gcd_ticks` (29 ticks).

### 2. Phased simultaneous resolution (order-independent)

Within `Arena::advance`, Phase 4 resolves combat in three deterministic sub-phases
using `casting::CastingState` and `combat_state::CombatState` against one shared
post-movement snapshot (`pre_combat`):

- **4a. Cancellations & active casts:** Explicit `CastAction::Cancel` cancels an
  active cast; remaining casts validate caster state (alive, not locked out, not
  moving for stationary casts) and, on the completion tick (`remaining_ticks == 1`),
  re-validate target life, team, range, line of sight and costs. Completed casts
  pay their costs, queue effects and start their personal cooldown; failed casts
  emit `CastInterrupted` without starting personal cooldown or spending any cost,
  and refund the GCD they started (see the 2026-11-22 and 2026-11-26
  amendments).
- **4b. New cast intents:** Each actor's `CastAction::Cast` validates through
  `CastingState::try_cast`. GCD starts at cast start; a timed cast that does
  not complete refunds it (2026-11-26 amendment). Costs are checked at start; instants pay them at once and timed casts
  pay them only on completion. Instant interrupts (`Quell`) only affect targets
  that remained actively casting after 4a, so cancelling via movement or
  `CancelCast` on the same tick successfully fake-casts (baits the interrupt
  without suffering lockout), and actor index ordering never breaks ties.
- **4c. Simultaneous application:** Interrupt lockouts, incoming damage, healing
  (via `effects::apply_heal`, never reviving dead fighters), death-interrupts
  (`InterruptReason::CasterDead`) and round completion apply together.
- **Phase 6:** After vertical motion, `gcd_remaining`, `lockout_remaining` and
  per-spell `cooldowns` decay by 1 tick and alive fighters regenerate resource on
  every third tick.

### 3. Protocol and state-hash contract

- `Combatant` stays `Copy` and zero-allocation, carrying `resource`,
  `gcd_remaining`, `lockout_remaining`, `cooldowns: [u32; 4]` and
  `active_cast: Option<ActiveCastState>`. `Arena::hash_state` covers every field,
  and `Arena::restore` / `ServerMessage::decode` reject out-of-bounds health,
  resource or malformed `active_cast` invariants.
- `arena-protocol` bumps `PROTOCOL_VERSION` to `3` and `RULES_VERSION` to `4`,
  replaces `InputFrame.attack` with `InputFrame.action: WireAction`, adds
  `CastStarted`, `CastCompleted`, `CastInterrupted` and `Healed` events, and
  mirrors all new `Rejection` variants exhaustively.

### 4. Client prediction decision

- **Own movement, facing and jumping remain client-predicted** (`Predictor`).
- **Casting, cooldowns, GCD, resources, interrupts, damage and healing are NOT
  client-predicted.** They depend on other actors' positions, line of sight,
  health and simultaneous interrupts; predicting them locally would create
  phantom cast bars and phantom resource dips on rejection or interrupt.
- In `arena-client::network_receive`, predicted state overrides only the local
  fighter's spatial fields (`position`, `height`, `vertical_velocity`, `momentum`,
  `facing`), keeping combat fields strictly server-owned.

## Consequences

- Server authority now covers cast times, GCD, per-spell cooldowns, resources,
  lockouts and interrupts with identical rules across headless simulations,
  match servers, bots and the browser client.
- The bridge (`arena-client` `publish` and `web/src/bridge/snapshot.ts`) exposes
  authoritative `maxResource`, per-unit `resource`, `gcdRemaining`,
  `lockoutRemaining` and `cast`, paired with pure TypeScript helpers in
  `web/src/domain/casting.ts`. Rendering cast bars and live resource meters in
  the React HUD is queued as a separate Lane B slice requiring human browser
  review.

## Amendment (2026-11-22): costs are charged when a timed cast completes

**Status: proposed (implemented in `arena-sim`, `RULES_VERSION = 6`).**

Playtesting found that losing a cast to an interrupt, a cancel, movement or a
failed completion re-check also lost its whole resource cost, which felt like a
double penalty on top of the lost GCD and (for interrupts) the lockout.

- `CastingState::try_cast` still checks resource and health costs before a cast
  starts, so an unaffordable spell is rejected with `NotEnoughResource` /
  `NotEnoughHealth` exactly as before.
- Instant spells pay at once, unchanged.
- Timed casts pay nothing at start. `CastingState::tick` re-checks the costs on
  the completion tick, after the target, range and line-of-sight re-check, and
  charges them only if the cast resolves. A cast that is interrupted, cancelled,
  moved out of, or fails any completion re-check spends nothing. The GCD it
  consumed was not refunded until the 2026-11-26 amendment.
- **Health costs follow the same rule.** They are checked at start, re-checked at
  completion (still leaving at least 1 HP), and charged on completion. Keeping one
  rule for every cost means a cast never takes something the player cannot get
  back if it fails. No current spell has a health cost; the rule is covered by
  `casting` unit tests.
- A cost that became unaffordable mid-cast fails the completion with the existing
  `InterruptReason::Cancelled` wire reason. Nothing in the current kit can lower
  a caster's pool mid-cast, and only damage taken earlier in the cast can make a
  health cost unaffordable (no current spell has one), so no new wire reason was
  added; add one with a protocol bump if a drain effect ever lands.
- The client needs no separate parity code: practice mode and HUD pre-checks run
  the same `arena-sim` rules, and casting stays unpredicted (section 4).
- `RULES_VERSION` moves from 5 to 6 because replays of interrupted casts now end
  with different resource totals.

Balance consequences to expect in playtests:

- Passive regeneration during a cast is no longer "banked" into the space the
  cost freed up. A Rift Bolt started from a full pool now ends at 65 rather than
  80, because the pool sat at its cap for the whole cast. Casting from a full
  pool is slightly worse; casting from a part-spent pool is unchanged.
- Fake-casting (section 2) now costs only the GCD, not the spell's resource, so
  baiting interrupts is cheaper.

## Amendment (2026-11-26): only completed casts keep their GCD

**Status: accepted (implemented in `arena-sim`, `RULES_VERSION = 7`)**, at the
user's request: the GCD is for spells that finish successfully. The user
playtested it and accepted it, free fake-casting included, on 2026-11-27.

- The GCD still starts when a cast begins, so instants and casts that complete
  behave exactly as before. Nothing else can be cast during a timed cast, and
  the normal 29-tick GCD is shorter than the 45-tick Rift Bolt and Mend casts,
  so a completed cast's GCD has already run out when it lands.
- A timed cast that ends any other way refunds the GCD it started: an explicit
  `CancelCast`, moving during a stationary cast, a failed completion re-check
  (target, range, line of sight, costs), an interrupt and its lockout, and
  caster death. `Combatant::end_cast_early` is the single path for all of them,
  mirrored by `CombatState::interrupt_cast`.
- A GCD-triggering cast can only start when the GCD is zero and blocks every
  other spell while it runs, so the whole running GCD is the cast's, and the
  refund sets it to zero. An off-GCD timed cast (none exists yet) leaves an
  earlier spell's GCD running.
- `RULES_VERSION` moves from 6 to 7: replays with a cancelled or interrupted
  cast reach the next spell sooner.

**Fake-casting is free, by design** (decided by the user, 2026-11-27). With
the 2026-11-22 amendment a cancelled cast already spent no resource, so a
player can start and cancel casts back to back to bait an interrupt without
losing anything; interrupts still cost the interrupter its cooldown and
resource. Considered and not adopted: a short cast-start lockout after a
cancel, and keeping the GCD for casts the caster cancels while refunding it
only for interrupts.
