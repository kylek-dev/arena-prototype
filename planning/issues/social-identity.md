Give players a persistent identity outside the match: accounts, a chosen `Name#1234` handle, friends and invites, and an admin role. This is a social layer that sits beside the match server, not inside the simulation or match protocol.

**Status (2026-11-25):** step 1 is done. See the
[research note](../../docs/research/accounts-2026-11-25.md) and the proposed
[ADR 0055](../../docs/decisions/0055-accounts-oauth-login-in-arena-door.md):
OAuth login with Discord, Google or GitHub, handled by `arena-door`; no
passwords; a minimal SQLite store; guests stay. Step 2 (the identity service, with the
account id in tickets) is implemented on `auto/identity-service`. The login
UI is queued as a Lane B follow-up. Guests' temporary handles come with
step 3, the handles themselves.

Acceptance criteria, in shipping order (each step is its own PR):

- **1. Accounts research and decision (do first):**
  - Compare hosted and self-run login options for cost at playtest scale (tens of users) and at modest growth (thousands): for example plain OAuth/OIDC against Discord, Google or GitHub with our own session cookie; hosted services such as Auth0, Clerk, Supabase Auth or WorkOS; and self-hosted Ory or Keycloak. Cover price, free-tier limits, lock-in, data we would have to store, account deletion, and the operational load on the single-host deploy in `deploy/`.
  - Record findings in a dated `docs/research/` note with sources and a `Proposed` ADR. Don't build a custom password store unless the research shows it is worth it.
- **2. Identity service and guest fallback:**
  - A small identity/social service behind `arena-door` (separate module or crate, so accounts can grow later without touching `arena-sim` or match sessions).
  - Login yields an account id that `arena-door` puts into signed match tickets. The match server trusts only the ticket, never a client name claim.
  - Guests can still play: they get a temporary handle until they log in.
- **3. Display names (`Name#1234`):**
  - Player picks the word; the server assigns the number so the full handle is unique (similar in shape to other platforms' tag systems, with our own rules and UI).
  - Bounded length and character set, a basic blocked-word list, rate-limited renames.
  - Shown on nameplates, unit frames, scoreboard, custom lobby slots and spectator HUD. Protocol bump with decode limits and tests.
- **4. Friends and invites:**
  - Send, accept, decline and remove friend requests by handle; online/in-match presence.
  - Invite a friend to a party or a custom lobby with one click. The friend sees an accept/decline prompt. Party/lobby codes and links keep working as a fallback.
  - Per-account rate limits on requests and invites; tests for spam, self-invite, blocked and offline cases.
- **5. Admin role:**
  - A server-side role on the account, granted by deploy configuration (an operator-supplied account id or env var). Never hard-code a personal email or id in the repo.
  - Visible admin tag next to the name.
  - Admin-only features are gated on the server, not only hidden in the UI. Start with the map editor; leave room for more tools.

Out of scope here: chat (see `chat`), match history and rankings.
