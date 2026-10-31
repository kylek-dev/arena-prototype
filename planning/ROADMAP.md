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
