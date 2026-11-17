# ADR 0047: Accepting found matches, one queue per size, and queue status

Date: 2026-11-17. **Status: accepted and implemented**, with automated
tests. Human browser review of the queue bar and the match-ready window is
pending ([UI review](../UI-REVIEW.md)).

Amends [ADR 0046](0046-queues-parties-and-home-screen.md): it replaces the
solo 2v2 and solo 3v3 queues and the automatic ready check.

## Context

After trying the queues, the user asked for three changes:

- **Accept the match.** A clear "match ready" notification for everyone,
  with 15 seconds to accept. Whoever does not accept is removed from the
  queue, and everyone who accepted goes back in.
- **One queue per size.** No separate solo 2v2 and solo 3v3 queues, since
  they are the same as 2v2 and 3v3.
- **Clear queue status.** A more polished, clear view of waiting in the
  queue: how long you have waited, and an expected wait.

## Decision

### Three queues

- `1v1`, `2v2` and `3v3` (user decision, 2026-11-16).
- 2v2 and 3v3 take players on their own and parties up to the team size,
  and fill each team from whoever is queued, as in ADR 0046.
- Solo players and parties share one queue per size.

### Accepting a match

- **The offer.** When a queue can fill a match, everyone in it is offered
  the match. A `ready_check` message carries how many accepted out of how
  many, whether this player accepted, and the time left. It is sent again
  to everyone whenever someone accepts.
- **Time limit.** Players have 15 s (`DoorConfig::ready_timeout`).
- **Declining** ends the check for everyone at once.
- **Not accepting.** A player who declines, or does not accept in time,
  leaves the queue but stays connected, with their party intact. The
  player is told `declined` or `not_accepted`.
- **Parties.** A party plays together, so it leaves the queue with any
  member who did not accept. The other members are told
  `member_declined` or `member_not_accepted`.
- **Requeueing.** Parties whose members all accepted return to the front
  of the queue in their order, and keep their original search start.
- The game no longer accepts on the player's behalf. Bots still accept
  every match.

### Queue status and the expected wait

- **Status updates.** While searching, `queued` updates (every 2 s, and on
  queueing) carry: how many are in the queue, how long this search has run
  as the door measures it (so everyone in a party sees the same time), and
  the expected wait.
- **Expected wait.** The median of how long the last 20 matches in that
  queue took to find, from each party's first queueing to everyone
  accepting.
- **Too few matches.** Until a queue has 3 such waits, the expected wait
  is `null`, and the game says there are not enough recent matches to
  estimate rather than inventing a number. The history lives in the door's
  memory and starts over when the door restarts.

### Browser

- **Queue bar.** A bar at the top of the screen, on the home screen and in
  the arena alike, shows:
  - the queue and a pulsing marker;
  - a live timer of the time waited;
  - the expected wait (rounded: "under a minute", "about 3 min");
  - the players in the queue;
  - Cancel.
  - After a failed ready check it says the player is back at the front of
    the queue, or explains why they left it.
- **Match-ready window.** A centered window that takes the keyboard. It
  shows:
  - "Your 2v2 match is ready";
  - a 15-second countdown with a bar;
  - one marker per player, filled as players accept (how many, not who);
  - Accept (focused) and Decline. After accepting, it shows how many are
    still to accept.
  - While the tab is in the background, the page title counts down
    ("(12) Match ready!").
- **Play online panel.** It keeps the party controls and the three queue
  buttons. It only explains refusals of its own requests; the queue bar
  tells the rest.
- **Not included.** No sound and no system notification yet. Both need a
  decision about assets and browser permissions.

## Verification (2026-11-17)

- **Matchmaker tests.**
  - Everyone sees the ready check fill up, with the time left, and the
    match starts.
  - A decline ends the check at once, and those who accepted go first.
  - Players who do not accept in time leave the queue but stay connected.
  - A party leaves with a member who declined.
  - The expected wait is the median of recent waits and needs 3 samples.
  - A failed check keeps the search start.
  - The randomized test of 30,000 requests (now including declines)
    checks every index after each step.
- **Door integration tests (WebSockets).**
  - Four players are offered a 2v2 match, accept, and each joins their own
    seat. Before any match, the expected wait is `null` and the time waited
    is reported.
  - Friends in a party share a team.
  - Players who do not accept leave the queue and can queue again on the
    same connection, while those who accepted are requeued.
  - Declining ends the check for everyone.
  - Refusals are checked as before.
- **End to end (release builds, no browser).** A scripted client with a
  browser `Origin` and three `arena-bot --queue 2v2` bots were offered a
  match. The client saw "accepted" go from 0 to 3 of 4, accepted, and
  joined. All four seats joined, with 592 cross-seat state checks, 0
  mismatches and no errors.
- **Browser tests.** 241 JavaScript tests pass. They cover:
  - the message parser and wording (timer, expected wait, countdown);
  - the runtime over fake sockets (accept, decline, requeue, the door's
    time waited);
  - the queue bar;
  - the match-ready window: countdown, acceptance markers, Accept focused,
    Decline, requeue and declined messages.
- **Not observed.** A browser showing the queue bar or the match-ready
  window.
