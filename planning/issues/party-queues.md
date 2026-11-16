Premade **2v2 and 3v3** queues: friends form a party and queue together against another party of the same size.

Depends on `matchmaking-queues` (queue and in-game join) and `custom-matches` (inviting players and lobby state, reused for parties).

Acceptance criteria:
- **Forming and running a party:**
  - A player forms a party of 2 or 3 by invite code.
  - The party leader queues for the matching mode, and every member enters or leaves the queue together.
  - A member who disconnects or leaves takes the party out of the queue.
- **Matching:** whole parties are matched against parties of the same size first. Whether solo players may fill a partial premade team (for example a party of 2 plus one solo in 3v3) is a decision to record.
- **Separate queues:** party queues stay separate from the solo queues (`solo 2v2`, `solo 3v3`), so solo players never face full premades.
- **Tests:** party queueing and matching, and a member dropping during queue or match formation. Human browser review.
