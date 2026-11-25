# Roadmap

Source plans live here; GitHub Issues hold discussion and execution state. GitHub Projects is a view over those issues, not the sole record.

| Stage | Outcome | Exit evidence |
| --- | --- | --- |
| 0: foundation | Original-IP direction, stack proposal, headless rules, public collaboration | Local checks, published docs and issues |
| 1: browser spike | One character, camera, movement, target and action in an original graybox | Named browser/device matrix, load and frame measurements |
| 2: network duel | Two browser clients, authoritative server, validated commands | Latency/jitter tests, disconnect and malformed-input tests |
| 3: combat slice | Two kits, cast/interrupt, defense and line of sight | Repeatable headless scenarios and useful human playtest |
| 4: team play | 2v2, round flow, rematch, simple guest lobby | Four-player sessions and structured feedback |
| 5: identity and persistence | Original art direction, accounts and match history | Provenance inventory and operational recovery checks |

Stage 0 is published. Stage 1 now has an implemented Bevy browser practice client with shared rules, a DOM HUD, and automated browser checks. Hardware performance, Safari coverage, and a real network/load baseline remain open; see docs/research/browser-spike.md. The table does not commit to a release date.

**Status update (2026-11-15).** Stage 2 is largely implemented ([ADR 0044](../docs/decisions/0044-authoritative-match-server.md)): an authoritative match server with versioned protocol and signed tickets, browser network play with own-movement prediction, rejoin after disconnects, bots measuring latency, jitter, drop-outs and load, and malformed-input tests. Two browser clients played a match, tested by the user on one machine. Rounds end in a draw after 20 minutes. Still open before Stage 2 exits: a remote playtest over real networks and a cross-browser review. Remote playtest tooling is ready ([ADR 0045](../docs/decisions/0045-front-door-and-playtest-deployment.md)): a front door with a lobby that hands out join links, origin and per-address limits, and a container image with a TLS proxy setup tested locally. It has not been deployed to a public host yet.

Next work, as issue definitions in `planning/issues/` (indexed in `planning/issues.json`; not yet published as GitHub issues):

In recommended order (2026-11-15):

| Order | Item | Stage | Definition |
| --- | --- | --- | --- |
| 1 | Deploy the match server for remote playtests (tooling done; a public host and playtest remain) | 2 | [remote-playtests](issues/remote-playtests.md) |
| done | Queue for 1v1, 2v2 and 3v3 from inside the game, accepting found matches (browser review pending) | 4 | [matchmaking-queues](issues/matchmaking-queues.md) |
| done | Queue premade parties in 2v2 and 3v3 (pulled forward by the user; browser review pending) | 4 | [party-queues](issues/party-queues.md) |
| done | Show every player's chosen look in matches (moved up by the user; browser review pending) | 4 | [networked-appearance](issues/networked-appearance.md) |
| done | Show the round clock and settle the time-limit result (browser review & playtest tie-break tuning pending) | 3 | [round-clock](issues/round-clock.md) |
| done | Show a scoreboard with per-player stats, Leave and Queue again when a match ends (browser review pending; CC columns wait for CC effects) | 4 | [scoreboard](issues/scoreboard.md) |
| done | Host private custom matches with invites, teams, versioned settings, spectator slots and rematch (browser review pending) | 4 | [custom-matches](issues/custom-matches.md) |
| done | Make cast times, cooldowns, resources and interrupts server-authoritative ([ADR 0048](../docs/decisions/0048-server-authoritative-casting-and-resources.md)) | 3 | [server-casting-resources](issues/server-casting-resources.md) |
| done | Spectate matches live (optional configurable delay), free/follow camera and watch-only HUD ([ADR 0050](../docs/decisions/0050-spectating-and-anti-ghosting-delay.md); browser review follow-up pending) | 4 | [spectating](issues/spectating.md) |
| 2 | Production-grade observability: OpenTelemetry logs, metrics, tracing & self-hosted Grafana LGTM stack (local and prod); Phase 1 done ([ADR 0054](../docs/decisions/0054-observability-logs-metrics-and-local-lgtm.md)) | 2 / 5 | [observability-telemetry](issues/observability-telemetry.md) |
| 3 | Accounts (researched login choice), `Name#1234` handles, friends and friend invites, admin role (added by the user 2026-11-22) | 5 | [social-identity](issues/social-identity.md) |
| 4 | Team, match and public chat with system messages and slash commands (`/help`, commands to open panels) (added by the user 2026-11-22) | 4 / 5 | [chat](issues/chat.md) |
| ongoing | Review the network match experience across browsers (manual, by the user) | 2 | [network-ui-review](issues/network-ui-review.md) |
| ongoing | Measure and harden netcode on real networks | ongoing | [netcode-hardening](issues/netcode-hardening.md) |

Why this order:
- **Queues first.** They replace copying join links and are what makes repeated playtests practical. Parties followed at once (user request, 2026-11-16).
- **Looks next.** The user wants players to set up their character before queueing and others to see it.
- **The round clock right after.** It is small, and players who meet through a queue should see the time limit coming.
- **Custom matches after parties.** They can reuse the party invites and lobby state.
- **Server-authoritative casting before spectating.** Spectator views then show cast bars and resources from the start instead of adding them twice.
- **Observability (OpenTelemetry + self-hosted Grafana LGTM).** Can be pulled forward alongside remote playtests: Phase 1 (structured `tracing` logs, lock-free tick histograms, `arena-door` `/metrics`, and an opt-in local `grafana/otel-lgtm` Compose profile) immediately helps diagnose real WAN playtests, while Phase 2 (OTLP distributed tracing, Alloy log/metric shipping, alerts) and Phase 3 (scrubbed browser RUM + Pyroscope profiling) harden production operations with $0 software cost.
- **Note on stages.** This pulls part of Stage 4 (lobby and queueing) ahead of the Stage 3 combat slice because remote playtests need it. The stages still describe the exit evidence for each milestone.

## Art production track (added by the user 2026-11-23)

This runs alongside the stages above. The quality bar and scope are in [ADR 0052](../docs/decisions/0052-art-scope-and-quality-bar.md): every shipped piece is finished to the highest quality, from one developer on a small tool budget.

| Order | Item | Target |
| --- | --- | --- |
| 1 | Gladiator vertical slice on Meshy Pro: human male and female bodies, Gladiator base gear set, sword and shield, 8 spell icons, in game ([ART-PRODUCTION.md](../docs/ART-PRODUCTION.md)) | 1 class slice |
| 2 | Base bodies and customization: 2 genders × at least 3 races, faces, hair, colors on the shared kit skeleton | 6+ bodies |
| 3 | Class identity: base equipment that represents each class | 5+ classes |
| 4 | Spells: 8+ per class, each with an icon and an animation (procedural where it meets the bar) | 40+ spells |
| 5 | Maps: a pool of 5+ arenas, each richly dressed with props and clear combat lanes | 5+ maps |
| 6 | Progression and store equipment per class (building the store and progression systems waits for the core loop) | open |

Each item exits with human review at combat distance and close up, and with recorded provenance and licensing for every asset.
