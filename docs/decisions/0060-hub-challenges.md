# ADR 0060: 1v1 challenges in the hub

Date: 2026-11-28. **Status: proposed (implemented)**, at the user's request
(`planning/BACKLOG.md`: "Challenge to a 1v1 from the unit menu ... It should
start a 1v1 match between the two players").

Builds on [ADR 0046](0046-queues-parties-and-home-screen.md) (the queue
socket and parties), [ADR 0056](0056-social-hub.md) (the social hub) and
[ADR 0057](0057-hub-on-the-world-server.md) (hub worlds on the world server,
placed by the door).

## Context

The unit menu offers **Challenge to a duel** as a greyed placeholder. Players
who meet in the hub want to fight each other directly, with a proper start,
end and result, without forming a custom lobby or queueing and hoping to be
matched.

A 1v1 match already has all of that: a countdown, the round clock, a result
and the scoreboard, and the page returns players to the hub afterwards. Hub
worlds themselves are friendly (ADR 0057); fighting inside one would need new
rules for who may hurt whom, an arena space inside the plaza, and a result
the world server does not keep.

## Decision

- **A challenge starts an ordinary 1v1 match.** On acceptance the door issues
  tickets for a `1v1` with default settings, the challenger in seat 0, the
  challenged player in seat 1, exactly as a queue match is issued. Both leave
  the hub for the match (`hub_left` `match`) and return through the page as
  after any match. No new match type, rule set or result path. The metric
  `arena_door_challenge_matches_total` counts them; they count towards
  `arena_door_matches_issued_total`, not the queue's.
- **Between two players in one hub world.** The door finds the target by
  handle among the players it has placed in the challenger's world (confirmed
  placements only), case-insensitively. Guests may challenge and be
  challenged: the hub admits them and no account is needed to play a match.
  A name two players in the world share (guest handles may repeat), a player
  either side blocked, oneself and anyone elsewhere all get the same
  `no_such_player`, so nothing about blocks or other worlds leaks.
- **Kept by the matchmaker.** The matchmaker already knows who is searching,
  in a ready check or in a custom lobby, and stays clock-free and
  unit-tested (`matchmaker_challenge.rs`). It holds handles the door gave
  it, as it names nobody itself.
- **Limits.** One open challenge per player (a new one withdraws the old;
  `withdraw_challenge` takes it back), at most 3 held from others and never
  two from players of one name (answers name the challenger), 30 s to answer, one attempt per 2 s per
  connection, spent before the door searches the world so a flood costs
  nothing.
- **Busy players.** A player searching, in a ready check or in a custom lobby
  cannot challenge. One who is busy when accepting is refused and keeps the
  challenge, to accept after cancelling. If the challenger has become busy,
  acceptance closes it for both (`busy`). Parties are left alone: party-mates
  are not pulled in and stay in the party.
- **Closing.** Declining tells the challenger; expiry tells both; leaving the
  game, leaving the world or being matched closes a player's challenges and
  tells the other player. The door checks that both players still share a
  world, and that neither has blocked the other since, on each 250 ms tick
  and again on acceptance. Two players who challenged each other play one
  match; the reverse challenge ends with it.
- **Wire.** Queue-socket JSON only (`challenge`, `answer_challenge`;
  `challenged`, `challenge_sent`, `challenge_ended`; see
  [PROTOCOL.md](../PROTOCOL.md)). The binary match protocol and the rules are
  unchanged, so `PROTOCOL_VERSION` and `RULES_VERSION` stay.

## Consequences

- The result is a normal match result. Challenge matches are not rated or
  recorded separately; a later ranking or record can tell them apart by
  adding the source to the ticket.
- The page offers **Challenge to a 1v1** in the unit menu (in the hub only)
  and shows challenges with the invitation cards: Accept and Decline for one
  received, Withdraw for one's own, and a notice of how either ended
  (`challengeText` in `web/src/domain/matchmaking.ts`); pending human review.
- A party-mate may still queue the party while one member plays a challenge,
  as they already may during a custom match; the ready check then reaches
  the player in the match.

## Alternatives considered

- **Fighting inside the hub world.** Needs per-pair hostility, a space and a
  result kept by the world server, for the same outcome a 1v1 match gives.
- **A custom lobby made for the two.** Lobbies are for hosts arranging teams
  and settings, and stay open for rematches; a challenge is a one-off
  question with one answer.
