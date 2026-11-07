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

Fast input tests: `npm test` or `npm run test:watch` with Node 24; rules tests: `cargo test -p arena-sim`. Automatic CI checks rules and input; full client builds are opt-in ([CI policy](docs/decisions/0005-lightweight-ci.md)). E2E review is manual; see [validation policy](docs/decisions/0004-unit-first-validation.md).

## Browser prototype

See [browser development](docs/BROWSER.md) for setup. With the pinned Rust/WASM tools installed:

```sh
./scripts/build-web.sh
npm run serve
```

Open http://127.0.0.1:8000. WASD moves; left-drag looks; right-drag turns; both mouse buttons walk and steer. Tab selects, 1 attacks, and R resets.

## License

Original code and documentation are MIT licensed; see [LICENSE](LICENSE). The optional downloaded character has separate terms and is excluded from public Git history; see [asset provenance](assets/README.md). The build uses a procedural fallback when that local download is absent. Asset contributions require explicit provenance and licensing. The project name is provisional and does not establish a final brand.
