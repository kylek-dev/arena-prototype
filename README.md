# Arena Prototype

Temporary project name. An original, browser-first competitive arena game, inspired by the tactical depth of team arena combat. All characters, worldbuilding, art, audio, and ability identities will be our own.

**Status:** browser practice prototype plus a shared, tested Rust simulation. Solo play against a stationary opponent; multiplayer is not implemented.

## Start here

- [Fresh-machine developer setup](docs/DEVELOPMENT.md)
- [Game direction](docs/GAME.md)
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

See [browser development](docs/BROWSER.md) for setup. With the pinned Rust/WASM tools installed:

```sh
npm ci
./scripts/build-web.sh
npm run serve
```

Open http://127.0.0.1:8000. WASD moves; Space jumps with takeoff momentum; left-drag looks; right-drag turns; both mouse buttons walk and steer. Tab selects, 1 attacks, and R resets.

The React overlay adds target/focus frames, action bars, an icon-only draggable Pulse spellbook, remappable bindings, quick binding, direct layout editing and local profiles. Escape opens the bindable game menu; Shift+Escape clears target. Ordinary HUD clicks retain gameplay focus; typing/dialogs/editing own their keyboard input. Shift+Tab enters normal UI navigation. Layout frames and the editor window are movable. The interface uses Radix Themes with a neutral dark palette. Only Pulse and one stationary opponent exist. Use `npm run dev` for fast UI iteration after the first complete build; `npm run build` rebuilds only the frontend. See [frontend boundaries](docs/FRONTEND.md). Human rendered review of the revised overlay is pending.

## License

Original code and documentation are MIT licensed; see [LICENSE](LICENSE). The optional downloaded character has separate terms and is excluded from public Git history; see [asset provenance](assets/README.md). The build uses a procedural fallback when that local download is absent. Asset contributions require explicit provenance and licensing. The project name is provisional and does not establish a final brand.
