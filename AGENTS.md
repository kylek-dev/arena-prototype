# Working on Arena Prototype

## Product intent

Build an original browser-first arena combat game. Arena Prototype is a placeholder name. Read README.md, docs/GAME.md, docs/ARCHITECTURE.md, and the applicable decision records before changing architecture.

## Working agreement

- Carry authorized tasks through implementation and relevant verification. Do not introduce repeated confirmation steps for ordinary reversible work.
- Keep code, game rules, docs, and operational instructions readable as text. Prefer focused patches and ordinary open source tools over editor-only workflows.
- Use pinned dependencies and their version-specific documentation. Do not guess APIs from a different Bevy release.
- Keep the simulation independent of graphics, OS clocks, network I/O, and model APIs. Pass commands and ticks explicitly. Specify command ordering.
- Server authority is the intended design: clients submit intent; the server validates results. Never trust client health, damage, cooldown, or position claims.
- Record significant choices as numbered decision records, marking proposals and accepted decisions honestly.
- Use original IP. Do not import Blizzard data, UI, names, art, audio, maps, protocols, or proprietary source. Record third-party and AI asset provenance.
- Keep secrets, credentials, personal data, build outputs, and large raw captures out of Git. Use synthetic fixtures.
- Do not claim a browser, network, CI, publishing, or performance check passed unless actually observed. Distinguish completed work from plans.
- Do not add runtime LLM dependencies to combat. AI development assistance, bots, and offline authoring are separate concerns.

## Verification

Run `./scripts/check.sh` for Rust changes. Add behavioral tests for game rules, exploit boundaries, replay order, and regressions. For client changes, also run the WASM Clippy check and browser checks documented in docs/BROWSER.md. Rendering changes need observed browser evidence. Use `cargo run -p arena-headless` for the scripted scenario.

## Repository map

- crates/arena-sim: pure fixed-tick rules and tests
- crates/arena-headless: executable scenario runner
- crates/arena-client: Bevy rendering and narrow browser bridge
- web: HTML/CSS HUD and browser input
- tests/browser: Playwright interaction checks
- docs/research: dated findings with source links
- docs/decisions: proposals, evidence, and accepted decisions
- docs/wiki: versioned wiki pages
- planning: roadmap and source issue definitions

Never silently change license terms or erase someone else's work. Keep GitHub issue and project state exportable; see docs/GITHUB.md.
