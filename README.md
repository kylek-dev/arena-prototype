# Arena Prototype

Temporary project name. An original, browser-first competitive arena game, inspired by the tactical depth of team arena combat. All characters, worldbuilding, art, audio, and ability identities will be our own.

**Status:** research and executable headless foundation. No playable browser client or multiplayer service yet.

## Start here

- [Game direction](docs/GAME.md)
- [Technology recommendation and alternatives](docs/decisions/0001-stack.md)
- [Wenilla investigation](docs/research/wenilla.md)
- [Architecture](docs/ARCHITECTURE.md)
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

## License

Original code and documentation are MIT licensed; see [LICENSE](LICENSE). No third-party game code or assets are bundled. Future asset contributions require explicit provenance and licensing in [assets/README.md](assets/README.md). The project name is provisional and does not establish a final brand.
