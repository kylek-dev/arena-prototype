# ADR 0055: Accounts log in through WorkOS AuthKit; the door keeps them in PostgreSQL

Date: 2026-11-25. **Status: proposed.** Revised the same day, before
release, at the user's request:
- Players must be able to log in without a Discord, Google or GitHub
  account.
- Accounts live in PostgreSQL, in a database of their own, not SQLite.

The first version (direct OAuth to Discord, Google and GitHub, with a SQLite
file) is kept in git history (branch `auto/accounts-decision`, PR #93).

## Context

[social-identity](../../planning/issues/social-identity.md) adds accounts,
`Name#1234` handles, friends and an admin role. Step 1 asked for a
researched login choice. The research note
[accounts-2026-11-25](../research/accounts-2026-11-25.md) compares:
- logging in directly against OAuth providers;
- hosted services (WorkOS, Clerk, Supabase Auth, Auth0);
- self-hosted servers (Ory Kratos, Keycloak);
- our own password store.

All are free at our scale, so the first version chose direct OAuth to avoid
lock-in and email. The user then added two requirements:
- **Login without a social account.** That means email and password,
  emailed codes, or passkeys, with password resets and email delivery.
  Building that ourselves is the liability the research advised against.
- **PostgreSQL for durable data.** The project expects to need PostgreSQL
  anyway, and one database technology is simpler to run than two. Each
  kind of data gets its own database.

## Decision

1. **Players log in through WorkOS AuthKit's hosted page.**
   - The page offers email + password, emailed one-time codes ("magic
     auth"), Google and GitHub, so no social account is needed.
   - Discord is not offered: AuthKit does not list it, and the user made it
     optional.
   - The door sends the browser to
     `https://api.workos.com/user_management/authorize?provider=authkit` with
     state and a PKCE S256 challenge.
   - At `/api/auth/callback`, the door trades the code at
     `POST /user_management/authenticate`, sending the PKCE verifier and the
     API key as `client_secret`.
   - Configuration: `ARENA_WORKOS_CLIENT_ID` and `ARENA_WORKOS_API_KEY`. The
     dashboard lists `https://HOST/api/auth/callback` as a redirect URI,
     `https://HOST/` as the default sign-out URI, and
     `https://HOST/api/auth/login` as the initiate-login URI.
2. **What WorkOS holds, and what we hold.**
   - WorkOS holds the player's email, password hash and login methods, and
     sends verification, code and reset emails.
   - We keep only the WorkOS user id (`user_...`), never the email, name,
     avatar or tokens.
   - From the access token we read only its `sid` (the WorkOS session id),
     to end that session at logout. The token comes straight from WorkOS
     over TLS in answer to our authenticated call, so its signature is not
     checked.
   - The privacy notice published before login must name WorkOS as the
     processor that holds emails.
3. **No password store, no email sending of our own.**
4. **Accounts live in PostgreSQL, in a database used for nothing else.**
   - The URL is `ARENA_DATABASE_URL`. In Compose this is the `identity-db`
     container: database and role `arena_identity`, private network only,
     `mem_limit` 256 MB.
   - The door owns the schema through numbered migrations, run on start
     under an advisory lock.
   - Tables:
     - `accounts`: our random id, the WorkOS user id, the session
       generation, timestamps, and the handle and its rename time.
     - `friends` and `blocks`, whose foreign keys cascade on deletion.
   - Writes that could race take locks:
     - Picking a handle takes a per-name advisory lock (the unique index is
       the backstop).
     - Friend actions lock both accounts' rows.
   - The connection is plain TCP on the private network. Using a managed
     PostgreSQL later needs TLS added.
5. **Sessions are our own.**
   - A `__Host-arena_session` cookie (`Path=/`, `HttpOnly`, `Secure`,
     `SameSite=Lax`) carries:
     - the account id;
     - its session generation;
     - an expiry;
     - the WorkOS session id.
   - It is signed with `ARENA_SESSION_KEY`, separate from
     `ARENA_TICKET_KEY`.
   - Every request checks that the account still exists at that generation.
     Deletion, "log out everywhere" and bans therefore take effect at once.
6. **Logging out ends both sessions.**
   - `POST /api/auth/logout` (and `logout_everywhere`) clears our cookie and
     answers `{"logout_url"}`: WorkOS's
     `/user_management/sessions/logout?session_id=...&return_to=PAGE`.
   - The page opens that URL so the hosted page does not sign the player
     straight back in.
   - "Everywhere" also raises the generation, ending our sessions on every
     device. WorkOS sessions on other devices end at their own expiry.
7. **The login flow cannot be forged.**
   - State and the PKCE verifier sit in a short-lived (10 minute) signed
     `__Host-arena_oauth` cookie and are checked at the callback.
   - A callback carrying WorkOS's `error` fails.
   - After login the player always returns to the configured game page;
     there is no free-form "return to".
   - Every cookie-authenticated `POST`, and the queue WebSocket handshake,
     must carry an allowed `Origin`. With `--any-origin`, cookie
     authentication is off.
8. **Tickets carry the account id.** `arena-door` signs the logged-in
   player's account id (and handle and admin tag) into match tickets; the
   match server trusts only the ticket.
9. **Guests stay first-class.** Without login, a player keeps a random guest
   id and a temporary `Guest#NNNN` handle.
10. **Deletion goes to WorkOS first.**
    - `POST /api/auth/delete` first deletes the WorkOS user
      (`DELETE /user_management/users/{id}`, where a 404 counts as done).
    - If WorkOS cannot be reached, nothing is deleted (`502 unavailable`)
      and the player can retry.
    - The door then deletes our row, and its friends and blocks by cascade.
    - Tickets already issued stay valid until they expire. Backups keep the
      row until they rotate out.
11. **The admin role comes from configuration** (`ARENA_ADMIN_ACCOUNTS`,
    our account ids), checked on each request and never stored.

## Alternatives

- **Direct OAuth to Discord, Google and GitHub** (this ADR's first version):
  no third party holds emails and Discord is included, but it cannot log in
  a player without a social account.
- **Our own email + password or passkeys:**
  - Passwords bring hashing, credential-stuffing defence, resets and email
    delivery, all ours to run.
  - Passkeys alone leave players without one stranded.
- **Clerk, Auth0, Supabase Auth:**
  - Clerk and Auth0 tie sessions and UI to their SDKs or hosted pages.
  - Supabase free projects pause after a week idle, which blocks logins.
- **AuthKit passkeys:**
  - They work only in the hosted page.
  - WorkOS advises a custom domain ($99/month) before enabling them in
    production, because a passkey is bound to the domain it was made on.
  - Left off for now. If enabled later, set up the custom domain first.
- **SQLite** (this ADR's first version):
  - One file, no extra service, and plenty for playtests.
  - But it pins accounts to one door process, and the project plans on
    PostgreSQL anyway.

## Consequences

- **Cost: $0 at our scale.** Pricing read 2026-11-25:
  - The first 1,000,000 monthly active users are free. A monthly active
    user is anyone who signs up, signs in or updates their profile in a
    calendar month.
  - Each further million costs $2,500/month.
  - Paid add-ons we do not use: custom domain $99/month; Radar bot
    protection (first 1,000 checks free, then $100/month per 50,000); SSO
    and Directory Sync $125 per connection.
  - Staging environments are free.
- **Lock-in, moderate.**
  - Leaving means exporting users: profile fields from the dashboard, and
    password hashes (bcrypt) only through a support request.
  - Our account ids stay ours. A move maps WorkOS user ids to the new
    provider's ids.
- **A WorkOS outage blocks new logins only.** Players already logged in keep
  playing, because the sessions are ours.
- **One more container.** `identity-db` (PostgreSQL 18.7) runs next to the
  door.
  - Back it up with `pg_dump` (`docs/DEPLOYMENT.md` "Accounts").
  - The door waits for its health check before starting.
  - Compose requires `ARENA_IDENTITY_DB_PASSWORD` even for a guest-only
    deploy.
  - Admin-saved maps moved to their own `--maps-dir`.
- **Tests need PostgreSQL.**
  - Account tests read `ARENA_TEST_DATABASE_URL` and give each door a fresh
    schema. Without it they skip and say so.
  - `./scripts/check.sh` starts a throwaway `postgres:18.7-alpine` container
    when the variable is unset, and fails if Docker is missing.
  - CI runs PostgreSQL as a service.
  - WorkOS is faked on loopback, so CI needs no WorkOS account.
- **No migration from the first version.** It was never released, so no
  SQLite data exists to carry over.
- **Login callbacks put a one-time code and state in the URL.** Caddy's
  access log drops both (`deploy/Caddyfile`).
- **Revocation reaches open queue connections.** Deleting an account or
  logging out everywhere tells the matchmaker to stop signing that account
  into tickets for connections that are already open.
