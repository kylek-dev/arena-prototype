# ADR 0006: Automatic full builds for relevant changes

Date: 2026-11-06. **Status: accepted**, following the user's request to automate full builds selectively. Supersedes ADR 0005's dispatch-only full-build policy, not its always-fast rules/input jobs.

## Decision

Keep `ci.yml` unchanged: required `rust` and `browser` checks run on every PR and main push without renderer compilation. Run `client-build.yml` on every PR and main push, retaining manual dispatch for release checks and arbitrary branches. Detect relevant changes inside the workflow and conditionally run its expensive jobs, rather than filtering out the entire workflow.

Trigger full native/WASM checking for changes to any Rust crate, Cargo manifest or lockfile, root Rust toolchain pin, `.cargo` configuration, assets, scripts, or workflow files. Shared simulation changes affect the client too; do not filter solely to `arena-client`. Workflow changes include Dependabot's Actions updates and must validate the build pipeline itself. Use the PR merge base or previous main tip with Git pathspecs; manual dispatch and new-branch pushes always build. Docs-only and JavaScript-only changes outside these build-input directories retain the fast path. Changes to documentation inside a watched directory may conservatively trigger a build.

Reuse existing pinned actions, tools, native/WASM caches, build commands, and artifacts. Cancel superseded builds per PR/ref. Do not add rendered tests, privileged PR triggers, new dependencies, or repository settings changes.

## Consequences

Bevy/Cargo updates can still take six minutes or more when rebuilding dependencies; that is intentional coverage rather than a unit-test regression. Ordinary docs/input iterations avoid that cost. Add an always-reported `client-build` gate to the main ruleset, bound to the GitHub Actions app. It requires successful change detection plus both successful builds when needed, or both intentionally skipped builds when not needed. Detection errors, build failures, cancellations, and invalid outputs cannot pass the gate. Preserve all other ruleset settings. Human browser review remains necessary for rendering/input integration and before release.

Using a full-history Git diff avoids the event path-filter API's 300-file limit. Synthetic Git fixtures test relevant/unrelated paths, dispatch/new-branch behavior, invalid revisions, and PR versus push comparisons. No hosted timing or browser pass is claimed by this decision.
