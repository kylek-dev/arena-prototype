Premade **2v2 and 3v3** queues: friends form a party and queue together against another party of the same size.

Depends on `matchmaking-queues` (queue and in-game join) and `custom-matches` (inviting players and lobby state, reused for parties).

Acceptance criteria:
- **Forming and running a party:**
  - A player forms a party of 2 or 3 by invite code.
  - The party leader queues for the matching mode, and every member enters or leaves the queue together.
  - A member who disconnects or leaves takes the party out of the queue.
- **Matching:** whole parties are matched against parties of the same size first. Whether solo players may fill a partial premade team (for example a party of 2 plus one solo in 3v3) is a decision to record.
- **Separate queues:** party queues stay separate from the solo queues (`solo 2v2`, `solo 3v3`), so solo players never face full premades. *(Superseded 2026-11-17: the user chose one queue per size, so solo players and parties share 2v2 and 3v3; see ADR 0047.)*
- **Tests:** party queueing and matching, and a member dropping during queue or match formation. Human browser review.

**Status (2026-11-16).** Pulled forward at the user's request and implemented together with `matchmaking-queues` ([ADR 0046](../../docs/decisions/0046-queues-parties-and-home-screen.md)). It did not wait for `custom-matches`: parties got their own invite codes.

User decision: partial parties are filled with other parties or solo players in the 2v2 and 3v3 queues, while solo queues stay solo-only.

Human browser review is pending.
