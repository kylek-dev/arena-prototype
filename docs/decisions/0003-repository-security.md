# ADR 0003: Repository contribution security

Date: 2026-11-01. **Status: accepted.**

The public repository has an owner and two write collaborators. Protect main with reviewed PRs, one independent write-access approval, stale-approval dismissal, latest-push approval, resolved conversations and up-to-date Rust/browser CI bound to the GitHub Actions app. Include administrators and block force pushes and deletion. A second collaborator must review owner-authored changes; do not routinely disable protections to merge.

Keep CODEOWNERS advisory to request owner attention without making owner-authored infrastructure changes impossible to approve. Do not require commit signatures yet: contributor signing setup has not been established. Do not introduce automatic merging or privileged contributor-code execution.

Reduce supply-chain risk with commit-pinned allowlisted Actions, read-only tokens, no persisted checkout credentials, bounded jobs, external fork run approval, dependency alerts and reviewed Dependabot updates. Enable private vulnerability reporting and retain existing secret scanning/push protection. Account security remains each collaborator's responsibility. These controls reduce accidental and unauthorized changes but cannot stop an administrator changing settings or a reviewer approving malicious code.

Live settings were read back through the GitHub API; operational details and verification paths are in [GitHub operations](../GITHUB.md). Workflow and policy changes must still pass their PR checks before merge.

References: [GitHub branch protection](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches/about-protected-branches).
