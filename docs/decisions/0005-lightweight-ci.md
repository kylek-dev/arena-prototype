# ADR 0005: Lightweight automatic CI; opt-in client builds

Date: 2026-11-06. **Status: accepted**, following the user's request to keep automatic CI fast after dependency updates. Supersedes ADR 0004's requirement to compile the client on every PR and main push; its unit-test and manual E2E policy remains unchanged.

The dispatch-only full-build policy below is superseded by [ADR 0006](0006-selective-client-builds.md), which adds automatic full builds for relevant changes while retaining the fast checks on every PR.

## Evidence

The unit-first change was not reverted by the Dependabot merges. However, ADR 0004 retained whole-workspace native Clippy, WASM Clippy, and the optimized web build. Updating Bevy invalidates compiled dependency caches, making those steps expensive even without rendered tests. In [run 42335547002](https://github.com/kylek-dev/arena-prototype/actions/runs/42335547002), the browser job took 6m39s; native Clippy alone took 2m30s and WASM Clippy 2m24s. Both restored older caches and recompiled Bevy 0.20.1. The Rust rules tests finished in under a second, and 24 JavaScript unit tests passed. This is compilation cost, not a return of automatic Playwright tests.

## Decision

- Automatic `CI` retains required job IDs `rust` and `browser`, without path filters or conditional skips.
- `rust` checks workspace formatting, lints/tests only `arena-sim` and `arena-headless`, and runs the headless scenario. It does not compile `arena-client` or Bevy.
- `browser` checks JavaScript syntax and runs `npm test` with Node 24. No dependency installation, Rust/WASM compilation, browser installation, or rendered tests are required.
- Move full native workspace checking, WASM Clippy, web compilation, and `browser-build` artifact production to the dispatch-only `client-build.yml` workflow. Keep existing dependency/action pins and security restrictions.
- Keep `scripts/check.sh` as the complete local native check. Client changes still need the documented full native/WASM checks and human browser review before merge/release; automatic CI no longer proves client compilation.

## Consequences and verification

Dependency updates cannot force routine CI to compile the renderer. A green automatic check establishes rules/input validation, not a successful client build, DOM wiring, or rendering. Run the manual full-client workflow on client/engine dependency changes and before release, and record its actual outcome along with human review. No new hosted CI timing or human browser pass is claimed for this workflow change.

Local verification: automatic workflow commands passed (formatting, focused Clippy, seven Rust rules tests, headless scenario, JavaScript syntax, and 24 input tests). Both workflow files parsed as YAML; trigger/job assertions confirmed the full build is dispatch-only and automatic CI retains both required jobs without renderer build commands. Hosted workflow execution remains pending.
