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

**Status update (2026-11-15).** Stage 2 is largely implemented ([ADR 0044](../docs/decisions/0044-authoritative-match-server.md)): an authoritative match server with versioned protocol and signed tickets, browser network play with own-movement prediction, rejoin after disconnects, bots measuring latency, jitter, drop-outs and load, and malformed-input tests. Two browser clients played a match, tested by the user on one machine. Rounds end in a draw after 20 minutes. Still open before Stage 2 exits: a remote playtest over real networks and a cross-browser review.

Next work, as issue definitions in `planning/issues/` (indexed in `planning/issues.json`; not yet published as GitHub issues):

| Item | Stage | Definition |
| --- | --- | --- |
| Deploy the match server for remote playtests | 2 | [remote-playtests](issues/remote-playtests.md) |
| Review the network match experience across browsers | 2 | [network-ui-review](issues/network-ui-review.md) |
| Make cast times, cooldowns, resources and interrupts server-authoritative | 3 | [server-casting-resources](issues/server-casting-resources.md) |
| Show the round clock and settle the time-limit result | 3 | [round-clock](issues/round-clock.md) |
| Show every player's chosen look in matches | 4–5 | [networked-appearance](issues/networked-appearance.md) |
| Measure and harden netcode on real networks | ongoing | [netcode-hardening](issues/netcode-hardening.md) |
