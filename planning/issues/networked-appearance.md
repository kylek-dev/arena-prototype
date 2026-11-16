Show every player's chosen character look in matches. Today only the local player (actor 0) gets the customizable kit model; everyone else is the procedural placeholder, and looks are never sent to the server. User decision: a look is locked once a player joins a match.

Acceptance criteria:
- Phase A (rendering): kit models and appearance per fighter, animated from each fighter's own movement; practice opponents get default or randomized looks. Measure draw calls and memory at 3v3 against the character startup budget (ADR 0035).
- Phase B (network): clients send look choices (kit, part and colour/variant identifiers; small and bounded, never free-form paths) with `Hello`; the server checks their size and shape, stores them per seat and relays every seat's look; clients check received looks against their kit catalog and fall back to a default.
- Protocol tests for malformed and oversized looks; human browser review of the result.
