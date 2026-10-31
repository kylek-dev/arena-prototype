Replace the minimal 1D duel assumptions with documented commands and fixed-step rules for the browser slice.

Acceptance criteria:
- Specify coordinates, units, targeting, range, collision and line-of-sight behavior.
- Define command ordering and tie resolution without accidental player bias.
- Add headless tests for movement limits, cooldown boundaries, cast interruption and invalid targets as each mechanic is implemented.
- Define versioned scenario/replay serialization and verify replay results.

Keep rules independent of Bevy and wall-clock time.
