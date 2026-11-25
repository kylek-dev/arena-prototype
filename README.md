# Arena Prototype

Temporary project name. An original, browser-first competitive arena game, inspired by the tactical depth of team arena combat. All characters, worldbuilding, art, audio, and ability identities will be our own.

**Status:** browser practice prototype plus a shared, tested Rust simulation. Local 1v1, 2v2 and 3v3 rosters with stationary non-player units; multiplayer is not implemented.

## Start here

- [Fresh-machine developer setup](docs/DEVELOPMENT.md)
- [Game direction](docs/GAME.md)
- [Art direction](docs/ART-DIRECTION.md)
- [Art production](docs/ART-PRODUCTION.md)
- [Technology recommendation and alternatives](docs/decisions/0001-stack.md)
- [Wenilla investigation](docs/research/wenilla.md)
- [Architecture](docs/ARCHITECTURE.md)
- [Build and play in the browser](docs/BROWSER.md)
- [Roadmap](planning/ROADMAP.md)
- [Published wiki](https://github.com/kylek-dev/arena-prototype/wiki) ([versioned source](docs/wiki/Home.md))
- [Public Project board](https://github.com/users/kylek-dev/projects/3)
- [AI and contributor instructions](AGENTS.md)
- [GitHub publishing and backups](docs/GITHUB.md)

## Run the foundation

Install the Rust toolchain pinned in `rust-toolchain.toml`, then:

```sh
cargo run -p arena-headless
./scripts/check.sh
```

The dependency-free simulation resolves a scripted duel at fixed ticks. It is a foundation for rule tests and replays, not a claim that the full combat or network design is implemented.

Run `npm ci`, then `npm test` or `npm run test:watch` with Node 24 for input, domain and focused DOM tests. `npm run test:input` remains dependency-free. Rules tests: `cargo test -p arena-sim`. CI checks rules and the frontend on every PR; full Rust client builds remain selective ([CI policy](docs/decisions/0006-selective-client-builds.md), [React ADR](docs/decisions/0007-react-combat-ui.md)). E2E review remains manual.

## Browser prototype

**Latest UI review (2026-11-08):** the user approved the current team practice modes/frames, nameplate labels/target highlights, timing overlay and frame-pacing correction and requested publication. This supersedes pending review notes for these changes; see [human review record](docs/UI-REVIEW.md#subsequent-changes).

**UI review:** the user verified and approved all implemented UI changes on 2026-11-08, including earlier pending revisions and world nameplates. This supersedes the earlier pending-review notes below. See the [human review record](docs/UI-REVIEW.md) for scope and evidence limits.

See [browser development](docs/BROWSER.md) for setup. With the pinned Rust/WASM tools installed:

```sh
npm ci
./scripts/build-web.sh
npm run serve
```

Open http://127.0.0.1:8000. WASD moves; Space jumps with takeoff momentum; left-drag looks; right-drag turns; both mouse buttons walk and steer. Tab selects, 1 attacks, and R resets. Escape → Practice mode selects 1v1/2v2/3v3. Team frames exclude yourself and appear on the left; opponents appear on the right. Mode changes restart the round; reset preserves mode. Human review of these additions is pending.

Escape → **Map editor** edits and creates arenas (pillars, starting rooms, spawns, decorative props, ground texture, floor and venue sizes) in a top-down view with live preview, undo and validation, and saves `assets/maps/<id>.json` when run with `npm run dev` or `npm run serve`. See [map editor](docs/BROWSER.md#map-editor).

Escape → **Lighting tweaks (temporary)** opens movable live controls for light strength/color/direction, background, exposure, saturation/contrast, shadow/midtone/highlight grading, tone mapping and bloom. No reload is needed while tuning; values are session-only, with Reset and copyable JSON. See [browser controls](docs/BROWSER.md).

The React overlay adds target/focus and team/opponent frames, action bars, an icon-only draggable Pulse spellbook, remappable bindings, quick binding, direct layout editing and local profiles. Settings → Weapons switches cosmetic Verdant Eclipse glaives/Aetherthorn staff and six energy colors, saved on this device. Escape opens the bindable game menu; Shift+Escape clears target. Ordinary HUD clicks retain gameplay focus; typing/dialogs/editing own their keyboard input. Shift+Tab enters normal UI navigation. Layout frames and the editor window are movable. The interface uses Radix Themes with a neutral dark palette. Pulse remains the only ability; non-player units are stationary. Use `npm run dev` for fast UI iteration after the first complete build; `npm run build` rebuilds only the frontend. See [frontend boundaries](docs/FRONTEND.md). Human rendered review of the new team frames and practice modes is pending.

The player defaults to the CC0 Quaternius female human with bun hair, eyes/brows and the Ranger armor set. Included takeoff, airborne and landing clips accompany idle, jogging and spell animations. Left/right use a [shared lateral-run strafe library](assets/animations/README.md) matched to the jog's cadence; backward still reverses the forward cycle. Run a complete build; `ARENA_RIGGED_CHARACTER=0` forces the procedural fallback. See [asset inventory and limitations](assets/quaternius/README.md) and [character setup](docs/BROWSER.md#quaternius-default-female-2026-11-14). Human rendered review is pending.

The character now uses bounded runtime textures (9.2 MB model rather than
57.7 MB), with more visible 0.30 s takeoff and 0.80 s landing phases. The player
is temporarily unequipped; weapon sources/preferences remain but no weapon is
spawned or requested. Entry-hitch improvement still needs human browser review.

## License

Original code and documentation are MIT licensed; see [LICENSE](LICENSE). Binary art has separate terms or unconfirmed publication licensing; see [asset provenance](assets/README.md). Quaternius base/animations/outfits/props are CC0; the locally retained races use QAL, not MIT/CC0, and their raw files are excluded from Git. Asset contributions require explicit provenance and licensing. The project name is provisional and does not establish a final brand.
