Let players talk to each other and run commands from a chat box.

**Status (2026-11-25):** the server side is implemented on
`auto/chat-server` (`docs/PROTOCOL.md` "Chat"): channels, whispers, server-set
senders, limits, filter, ignore, blocks and admin mutes. Still to do:
- the chat box (Lane B): implemented on `auto/chat-box`, pending human review;
- system messages and slash commands (Lane B): implemented on
  `auto/chat-commands`, pending human review (`system` stays reserved, so
  clients cannot post to it; the page writes its own system lines). The
  admin-only commands are `/mute`, `/unmute` and `/clearname`, checked by the
  door's admin routes; `/mapeditor` is open to everyone, as the menu's Map
  editor is, while its server storage stays admin-only (to confirm in review).
- a seat lasts the 21-minute in-match window even when the match ends
  sooner, so right after a short match `team` still means the old match
  team and a finished spectator is still confined; clearing seats needs the
  match server to tell the door a match ended.

Acceptance criteria:

- **Channels:**
  - Team chat (your team in a match, or your party/lobby), match-wide chat, and a public channel.
  - Spectators can only post to a spectator channel. Players never see it, so spectators can't feed information to players (ghosting).
  - Whispers to friends once `social-identity` friends land.
- **Server rules:**
  - Messages go through the server (front door or social service, not `arena-sim`). The server sets the sender's name and channel; the client only sends text and the channel it wants.
  - Length cap, per-connection rate limit, basic blocked-word filter, and mute/ignore per player. Tests for floods, oversized text and spoofed senders.
- **System messages:** a separate, styled channel for system events, such as a player joining or leaving, disconnecting or reconnecting, match found, round start and result, being kicked, made host, or a friend coming online. Only the server or client can send them; players can't fake one.
- **Chat UI:** a movable chat box that works with the layout editor. Enter opens it. While typing, keys go to the chat box, not to combat. Unread markers for each channel.
- **Commands:**
  - Slash commands handled by a client-side registry. `/help` lists every command with a short description, built from the same registry so it can't fall out of date.
  - Every existing panel can be opened by command (for example `/spellbook`, `/keybinds`, `/settings`, `/customize`, `/layout`, `/queue`, `/lobby`, `/scoreboard`, `/diagnostics`).
  - Admin-only commands (such as `/mapeditor`) are listed only for admins and also checked on the server.
  - Unit tests for parsing, unknown commands and the help output.
