# Security policy

Report suspected vulnerabilities privately using [GitHub private vulnerability reporting](https://github.com/kylek-dev/arena-prototype/security/advisories/new). Include the affected commit, reproduction steps, impact, and a minimal synthetic example. Do not put credentials, personal data, or exploitable details in public issues. There is no guaranteed response time or bug bounty.

The current main branch is the supported development version. This is a local browser prototype; server authority and multiplayer security are not implemented. Do not use it as a production competitive service.

Contributors must keep secrets out of commits and logs, retain pinned dependencies and lockfiles, and document asset provenance. Review dependency and workflow changes as executable code. Never add privileged `pull_request_target` execution of contributor code, secrets to fork builds, or runtime model dependencies to combat.

If a credential leaks, revoke or rotate it immediately; deleting the file does not invalidate a credential or remove it from Git history. Notify the maintainer privately before coordinating any history rewrite.
