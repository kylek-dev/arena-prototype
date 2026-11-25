# ADR 0055: Accounts log in with OAuth providers through `arena-door`

Date: 2026-11-25. **Status: proposed.**

## Context

[social-identity](../../planning/issues/social-identity.md) adds accounts,
`Name#1234` handles, friends and an admin role. Step 1 asks for a researched
login choice before any of it is built. The research note
[accounts-2026-11-25](../research/accounts-2026-11-25.md) compares:
- logging in directly against OAuth providers;
- hosted services (WorkOS, Clerk, Supabase Auth, Auth0);
- self-hosted servers (Ory Kratos, Keycloak);
- our own password store.

It compares them on price at tens and thousands of players, lock-in, stored
data, deletion and the load on the single-host deployment. Everything is free
at our scale except our own operations, so the choice turns on lock-in,
privacy and how much runs next to the match loop.

## Decision

1. **Players log in with Discord, Google or GitHub** through OAuth 2.0
   authorization code with PKCE. This is implemented in Rust in an identity
   module beside `arena-door`, which calls each provider's token and user
   endpoints over HTTPS. Each provider is enabled by deploy configuration
   (client id and secret in `deploy/.env`); an operator may enable any subset.
2. **Minimum scopes, permanent keys.** We ask Discord for `identify`, Google
   for `openid` and GitHub for no scope. The account key is the provider's
   permanent id:
   - Discord's user id;
   - Google's `sub`;
   - GitHub's numeric `id`, never the login name.

   One account belongs to one provider identity. There is no automatic
   merging (we hold no email to merge on). Linking a second provider to an
   account is out of scope for now.
3. **No password store.** We do not hold passwords or send email.
4. **We store the minimum** in one SQLite file:
   - our random account id;
   - provider and subject id;
   - the handle and its number, once step 3 adds them;
   - creation and last-login times;
   - a session generation counter.

   No email, provider token, avatar or real name is kept. Provider tokens
   are used once to read the subject id and dropped. A subject id tied to a
   handle is still personal data, so a privacy notice is published before
   login is offered.
5. **Sessions are our own.** A `__Host-arena_session` cookie (`Path=/`,
   `HttpOnly`, `Secure`, `SameSite=Lax`) carries the account id, its session
   generation and an expiry, signed with a new `ARENA_SESSION_KEY` kept
   separate from `ARENA_TICKET_KEY`. Every request that uses it checks that
   the account still exists and that the generation matches. Deletion,
   "log out everywhere" and bans therefore take effect at once.
6. **The login flow cannot be forged.**
   - State and the PKCE verifier sit in a short-lived (10 minute) signed
     `__Host-arena_oauth` cookie and are checked at the callback.
   - The redirect URI is fixed per provider
     (`https://HOST/api/auth/<provider>/callback`), and after login the
     player always returns to the configured game page; there is no
     free-form "return to".
   - Every cookie-authenticated `POST`, and the queue WebSocket handshake,
     must carry an allowed `Origin`; requests without one get no account.
   - With `--any-origin`, cookie authentication is off. A development setup
     names its page with `--allow-origin`.
7. **Tickets carry the account id.** `arena-door` signs the logged-in player's
   account id into match tickets; the match server trusts only the ticket.
   Tickets are signed, not encrypted, so their holder can read the id. That
   is only the player's own id; other players and spectators see handles,
   never account ids.
8. **Guests stay first-class.** Without login, a player keeps today's random
   guest id and plays as before. Step 3 gives guests a temporary handle until
   they log in.
9. **Deletion** removes the account row, its handle and friend edges, and
   ends its sessions at once.
   - Tickets already issued stay valid until they expire (at most the ticket
     lifetime, 2 hours by default).
   - Backups keep the row until they rotate out.
   - Account ids are never logged.
   - The provider account is the player's own; they can unlink our app
     there.
10. **The admin role comes from configuration** (`ARENA_ADMIN_ACCOUNTS`, a list
    of our account ids). It is checked on each request and never stored, so
    it cannot drift from the deploy.

## Alternatives

- **WorkOS AuthKit**: free to 1M monthly active users. It is the fallback if
  we later want email or passkey login without building it. Moving to it is
  not free: its users arrive with WorkOS subject ids, so existing accounts
  would need a mapping, for example by logging in once through the old
  provider to link.
- **Clerk, Auth0, Supabase Auth**: free at our scale but tie sessions and UI to
  their SDKs or hosted pages. Supabase free projects also pause after a week
  idle, which would block logins between playtests.
- **Keycloak**: roughly 0.5–2 GB of JVM memory, by community reports, next to
  the match server.
- **Ory Kratos**: light, but every screen would be ours to build.
- **Own passwords**: the most liability for the least benefit, since players
  already have one of the three accounts.

## Consequences

- **No new service or monthly cost.** Login adds an outbound HTTPS client
  (rustls with `ring`, system certificate store, so the image needs
  `ca-certificates`).
- **The door keeps durable state.** ADR 0046 already made it a single
  instance with in-memory queues; now it also keeps a SQLite file.
  - Compose gains a named `door-data` volume, writable by the image's
    `arena` user.
  - Backups use SQLite's online backup (`.backup` or `VACUUM INTO`), never a
    raw copy of a live database.
  - `docs/DEPLOYMENT.md` ("restart it at any time") and
    `docs/ARCHITECTURE.md` (which calls the door stateless) are updated when
    step 2 lands.
- SQLite calls block, so the door runs them off the async workers
  (`spawn_blocking` or one dedicated thread).
- **Operators must register apps.** Each provider needs an OAuth app with the
  callback URI above, and Google needs its app audience published. In
  Testing mode Google allows 100 listed test users, but basic identity
  scopes such as `openid` are open to anyone. Consent screens show a privacy
  policy link, so the privacy notice above ships with login. Without any
  provider configured the game stays guest-only, as today.
- **Tests need no real accounts.** They use a local fake provider, so CI
  works offline.
- Steps 2–5 of the issue build on this. The ticket claim format and the
  protocol change land with steps 2 and 3, with their own version bumps.
