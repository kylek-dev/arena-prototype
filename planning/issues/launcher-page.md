Replace the home screen with an out-of-game launcher page: the one place for account management, kept out of the game UI (requested by the user 2026-11-26).

Acceptance criteria:
- **The page:** what a visitor sees before entering the game, in place of the home screen. It is a page of its own, not an overlay on the arena.
- **Account management:**
  - Sign in or sign up (WorkOS AuthKit, [ADR 0055](../../docs/decisions/0055-accounts-oauth-login-in-arena-door.md)).
  - Play as a guest, with the guest's temporary `Guest#` name shown.
  - Choose or change the `Name#number` handle.
  - Delete the account, with a confirmation; log out and log out everywhere.
  - The privacy notice (`privacy.html`), linked from the page.
- **Entering the game:** one clear Enter (or Play) action. Today it opens the game as now; once [social-hub](social-hub.md) lands it enters the hub.
- **Nothing of this in game:** no account controls anywhere in the game UI (already true since `auto/game-launcher`); a link back to the launcher page from the game menu instead.
- **Copy and style:** formal, punctuated copy (BACKLOG 2026-11-25), Radix Themes and the palette variables.
- **Tests:** UI tests for each account state (loading, guest, logged in, no handle yet, accounts off) and the delete flow; human browser review (`docs/UI-REVIEW.md`, a `docs/BROWSER.md` scenario).

Open questions for the user:
- What the page shows besides accounts (news, patch notes, character preview).
- Whether parties and friends can be managed from the launcher page too, or only in the hub.
