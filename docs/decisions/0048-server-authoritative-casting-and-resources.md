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
  re-validate target life, team, range and line of sight. Completed casts queue
  effects and start their personal cooldown; failed casts emit `CastInterrupted`
  without starting personal cooldown or refunding resource/GCD.
- **4b. New cast intents:** Each actor's `CastAction::Cast` validates through
  `CastingState::try_cast`. Resource and GCD are consumed at cast start and are
  never refunded on interrupt. Instant interrupts (`Quell`) only affect targets
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
