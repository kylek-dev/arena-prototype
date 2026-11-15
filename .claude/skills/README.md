# Project agent skills

Third-party Bevy 0.20 skills copied unchanged from [chrisgliddon/bevy-skills](https://github.com/chrisgliddon/bevy-skills) at commit `475ed421de043b0a8c1d248243f1e937962dfd02` (2026-11-03), under the MIT license in [BEVY-SKILLS-LICENSE](BEVY-SKILLS-LICENSE). Installed 2026-11-14 at the user's request.

Installed: `bevy` (router), `bevy-core-concepts`, `bevy-ecs-components`, `bevy-ecs-queries`, `bevy-ecs-systems`, `bevy-pbr-materials`, `bevy-assets`, `bevy-animation`, `bevy-rendering`, `bevy-wasm-webgpu`, `bevy-cargo-features`, `bevy-testing`, `bevy-cameras`, `bevy-diagnostics-profiling` and `bevy-migration-0-19-to-0-20`.

Not installed, as they don't fit this project: voxel, physics (Rapier; the rules live in `arena-sim`), Bevy UI and accessibility (the HUD is React), input actions (browser input bridge), localization, audio, save/load, capture, porting, `bevy-vfx` (adds third-party particle crates), older migrations and `similarity-rs`. The router skill lists these, but they are absent here.

[AGENTS.md](../../AGENTS.md) and the pinned Bevy 0.20.1 sources take precedence over these skills. To update, re-copy the chosen folders from a newer commit, review the diff and record the new commit above.
