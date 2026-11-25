# Game direction

**Working title:** Arena Prototype. All mechanics below are proposals, not locked requirements.

## Core promise

Short, readable team fights won through positioning, target choices, coordinated pressure, defensive timing, and counterplay. Build our own world, characters, terminology, visual identity, and ability kits.

## Smallest useful playtest

Start with a 1v1 technical duel, then a 2v2 human playtest with two contrasting kits. Preserve the option of 3v3 after readability and pacing work. One small arena with line-of-sight blockers; WASD movement, mouse camera, explicit targeting, hotkeys, and remappable controls. Prototype a small toolkit: primary pressure, casted pressure, defense, interrupt, mobility, and one readable control effect. Validate each mechanic before adding another.

No gear advantage in the first playtests. Use equalized loadouts and fast rematches. Separate mechanical skill from unlock progression. Explore match duration, resource pressure, and anti-stall rules using data rather than cloning another game's numbers.

**Decided (2026-11-15):** a round lasts at most 20 minutes (`MAX_ROUND_TICKS`); a round still running then ends in a draw. This bounds stalled play and how long a server must keep a match. Shorter limits or tie-breaks (for example remaining health) remain open to tuning from playtests.

## Questions to answer through play

1. How much of the fun comes from targeting versus aiming? Start with targeted attacks plus line-of-sight and range checks; compare a limited directional ability later.
2. Does 2v2 produce enough teamwork without requiring a healer? Test sustain limits before committing to role composition.
3. How many buttons produce meaningful choices? Start small and count unused abilities and confusing deaths.
4. What makes this world memorable? Develop original races, classes, factions, silhouettes and materials; the art scope is in [ADR 0052](decisions/0052-art-scope-and-quality-bar.md).
5. What should players learn after a loss? Plan readable combat events, a death recap, and eventually replays.

## Effects terminology

The agreed terminology is **Effects**, with **Positive Effects** for beneficial effects and **Negative Effects** for harmful effects. Use these names instead of buffs/debuffs, boons/afflictions, or auras for this concept.

This establishes naming only. Effect mechanics, duration, stacking, removal, and presentation remain undecided; an Effects system is not implemented yet.

## Defer

Open world, MMO progression, loot economy, matchmaking ratings, guilds, mod scripting, mobile controls and runtime generative AI. These are outside the first playable milestone.

A store selling more epic class equipment and progression-based class equipment are planned ([ADR 0052](decisions/0052-art-scope-and-quality-bar.md)). Building them waits until the core loop is proven, and whether progression equipment affects stats is still open.

## Art and AI

Art is a primary goal: every piece that ships is finished to the highest quality ([ADR 0052](decisions/0052-art-scope-and-quality-bar.md)), following the [art direction](ART-DIRECTION.md). AI can help with code, tests, documentation and concept exploration. It can also produce production assets offline when a human directs the art, the result is finished to the bar, and provenance and commercial licensing are recorded. Procedural primitives and downloaded packs are placeholders until they are replaced or meet the same bar. Bots should consume ordinary game commands so AI-assisted testing remains independent of rendering or cloud model availability.
