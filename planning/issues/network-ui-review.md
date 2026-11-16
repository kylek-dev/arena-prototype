Review the network match experience in real browsers. The match UI, prediction and pacing were tested by the user in one browser on Windows against a WSL2 server; the pending checklist is in `docs/UI-REVIEW.md` (2026-11-15).

Acceptance criteria:
- Walk the checklist in current Chrome, Firefox and Safari where available: joining from links, the match banner states, greyed "Disconnected" frames and nameplates, Leave/Rejoin, reconnecting after dropping the connection, tab switching, and the diagnostics overlay.
- Record browser, OS and observed overlay numbers (corrections, queue, server ticks) in UI-REVIEW.md, distinguishing what was observed from what was not.
