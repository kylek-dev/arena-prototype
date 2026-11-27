import { Button, Text } from "@radix-ui/themes";
import { IDLE_QUEUE, noticeText } from "../../domain/matchmaking";
import { matchQueue } from "../../state/network";
import { createStore, useStore } from "../../state/store";

const NO_QUEUE = createStore(IDLE_QUEUE);
/** Party refusals answered here (joining by an old `#party=` link, leaving,
 * a full or busy party); the Friends list answers invitation refusals. */
const PARTY_REFUSALS = new Set(["no_such_party", "party_full", "party_busy", "rate_limited"]);

/** The party this player is in, with Leave party, on the launcher page.
 * Parties form by inviting friends; their codes stay internal. Nothing
 * while in a custom lobby or outside a party, except a party refusal. */
export function PartyStatus() {
  const queue = useStore(matchQueue, (q) => q);
  const state = useStore(queue?.state ?? NO_QUEUE, (s) => s);
  const party = state.party?.code ? state.party : null;
  if (!queue) return null;
  const refusal = PARTY_REFUSALS.has(state.notice ?? "") ? noticeText(state.notice) : null;
  const notice = refusal && (
    <div className="party-notice" role="alert">
      <Text size="1">{refusal}</Text>
      <Button size="1" variant="soft" color="gray" onClick={() => queue.dismiss()}>
        OK
      </Button>
    </div>
  );
  if (!party || state.lobby) return notice || null;
  return (
    <div className="party" role="group" aria-label="Party">
      <Text size="1">
        You are in a party of {party.members}.{" "}
        {party.leader ? "You lead the party." : "Another member leads the party."}
      </Text>
      <div className="party-actions">
        <Button
          size="1"
          variant="soft"
          color="gray"
          disabled={state.phase !== "idle"}
          onClick={() => queue.leaveParty()}
        >
          Leave party
        </Button>
      </div>
      {notice}
    </div>
  );
}
