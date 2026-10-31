# Wenilla investigation

Investigated 2026-10-31. Source checkout: `Arnesen/wenilla`, commit `ea514d8bb724c0571fc74183f23de7b1bbb303cf`. Findings below come from source files at that revision, not assumptions about a deployed service. No upstream code or assets were copied into this project.

## Actual stack

| Layer | Observed implementation |
| --- | --- |
| Language/build | Rust 2021 Cargo workspace; toolchain 1.98.1 |
| Engine | Bevy 0.18.1 ECS, rendering, input, and UI; wgpu 27 |
| Browser | wasm32-unknown-unknown, wasm-bindgen exactly 0.2.122, JavaScript/HTML canvas shell |
| Graphics | WebGPU required for the playable world; WebGL2 build reaches menu screens |
| Transport | Browser WebSocket to wenilla-host, then TCP to existing login/world servers |
| Web host | Axum 0.8, Tokio 1, Tower 0.5, tower-http 0.7; static files, compression, asset endpoints, proxy |
| UI compatibility | Existing FrameXML interpreted by custom Rust UI code; C Lua 5.1 compiled with wasi-sdk support |
| Assets | Custom readers for the legacy game formats; HTTP requests served from local game data |
| Physics/audio | Avian3D 0.6; patched Kira 0.12 and platform/browser audio backends |
| Realm service | Axum/Tokio, Askama templates, SQLx with SQLite/MySQL; session and administration code |
| Game authority | Existing 1.12.1-compatible server, commonly vmangos; not an original Rust game server |
| License | MIT OR Apache-2.0 for project code; no bundled Blizzard game assets |

## What to borrow

The design demonstrates Rust/Bevy in a browser, shared native/web code, narrow platform adapters, and inspectable source. Its JS bridge and automation API suggest a useful debugging/test interface for our own development builds.

## What to leave behind

A new arena game does not need WoW protocols, MPQ readers, FrameXML, a Lua compatibility VM, private-server administration, or client-install data. Build an original protocol and asset pipeline. A fork would commit us to compatibility work unrelated to the game.

## Caveats

The web README reports a single-threaded WASM client and CPU limits on its tested scene. Those are upstream measurements, not benchmarks for our game. Browser GPU support must be tested on named devices and browsers. A session-gated asset service does not itself grant distribution rights for third-party assets.

The checked-out browser crate still contains a comment describing older build defaults; prefer the current build script and web README over isolated comments. Pin revisions when reproducing results.

## Primary sources

- [Repository and README](https://github.com/Arnesen/wenilla/tree/ea514d8bb724c0571fc74183f23de7b1bbb303cf)
- [Workspace dependencies](https://github.com/Arnesen/wenilla/blob/ea514d8bb724c0571fc74183f23de7b1bbb303cf/Cargo.toml)
- [Browser architecture](https://github.com/Arnesen/wenilla/blob/ea514d8bb724c0571fc74183f23de7b1bbb303cf/web/README.md)
- [Host dependencies](https://github.com/Arnesen/wenilla/blob/ea514d8bb724c0571fc74183f23de7b1bbb303cf/crates/wenilla-host/Cargo.toml)
- [App dependencies](https://github.com/Arnesen/wenilla/blob/ea514d8bb724c0571fc74183f23de7b1bbb303cf/crates/benilla-app/Cargo.toml)
- [Realm dependencies](https://github.com/Arnesen/wenilla/blob/ea514d8bb724c0571fc74183f23de7b1bbb303cf/crates/wenilla-realm/Cargo.toml)
