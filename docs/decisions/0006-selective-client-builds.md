# ADR 0006: Automatic full builds for relevant changes

Date: 2026-11-06. **Status: accepted**, following the user's request to automate full builds selectively. Supersedes ADR 0005's dispatch-only full-build policy, not its always-fast rules/input jobs.

## Decision

Keep `ci.yml` unchanged: required `rust` and `browser` checks run on every PR and main push without renderer compilation. Add path-filtered PR and main-push triggers to `client-build.yml`, retaining manual dispatch for release checks and arbitrary branches.

Trigger full native/WASM checking for changes to any Rust crate, Cargo manifest or lockfile, root Rust toolchain pin, `.cargo` configuration, assets, scripts, or workflow files. Shared simulation changes affect the client too; do not filter solely to `arena-client`. Workflow changes include Dependabot's Actions updates and must validate the build pipeline itself. Keep push and PR filters identical. Docs-only and JavaScript-only changes outside these build-input directories retain the fast path. Changes to documentation inside a watched directory may conservatively trigger a build.

Reuse existing pinned actions, tools, native/WASM caches, build commands, and artifacts. Cancel superseded builds per PR/ref. Do not add rendered tests, privileged PR triggers, new dependencies, or repository settings changes.

## Consequences

Bevy/Cargo updates can still take six minutes or more when rebuilding dependencies; that is intentional coverage rather than a unit-test regression. Ordinary docs/input iterations avoid that cost. A successful rules/input run alone does not prove client compilation; reviewers must check the separate full-build result when it runs. Full-build jobs are not added to branch protection, because path-skipped required workflows can leave unrelated PRs blocked. Human browser review remains necessary for rendering/input integration and before release.

GitHub path filters have platform limits (including a 300-file diff limit); for unusually large PRs or uncertain coverage, dispatch the full workflow explicitly. No hosted timing or browser pass is claimed by this decision.
