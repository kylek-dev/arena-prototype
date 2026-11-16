import { Button } from "@radix-ui/themes";
import { useEffect, useState } from "react";
import { queueMessage } from "../../domain/matchmaking";
import { matchMessage } from "../../domain/network";
import type { Queue } from "../../runtime/matchmaking";
import type { Link } from "../../runtime/network";
import { matchQueue, networkLink } from "../../state/network";
import { engine } from "../../state/session";
import { createStore, useStore } from "../../state/store";
import { IDLE_QUEUE } from "../../domain/matchmaking";

const IDLE = createStore(IDLE_QUEUE);

function LinkStatus({ link }: { link: Link }) {
  const state = useStore(link.state, (s) => s);
  const joined = useStore(engine, (s) =>
    s?.networked && s.roster ? s.roster.filter((unit) => !unit.disconnected).length : null,
  );
  const seats = useStore(engine, (s) => (s?.networked ? (s.roster?.length ?? null) : null));
  const remembered = state.phase === "idle" || state.phase === "ended" ? link.remembered() : null;
  const message = matchMessage(
    state,
    !!remembered,
    joined !== null && seats !== null ? { joined, seats, seat: state.status?.seat ?? null } : null,
  );
  if (!message) return null;
  return (
    <div className={`match-status ${message.tone}`} role="status" aria-live="polite">
      <span>{message.text}</span>
      {message.action?.kind === "rejoin" && remembered && (
        <Button size="1" onClick={() => link.join(remembered)}>
          {message.action.label}
        </Button>
      )}
      {message.action?.kind === "dismiss" && (
        <Button size="1" variant="soft" color="gray" onClick={() => link.leave()}>
          {message.action.label}
        </Button>
      )}
    </div>
  );
}

/** `clock()`, refreshed every second while `ticking`. */
function useClock(ticking: boolean, clock: () => number) {
  const [now, setNow] = useState(clock);
  useEffect(() => {
    if (!ticking) return;
    setNow(clock());
    const id = window.setInterval(() => setNow(clock()), 1000);
    return () => window.clearInterval(id);
  }, [ticking, clock]);
  return now;
}

function QueueStatus({ queue }: { queue: Queue }) {
  const state = useStore(queue.state, (s) => s);
  const now = useClock(state.phase === "queued", queue.now);
  const message = queueMessage(state, now);
  if (!message) return null;
  return (
    <div className={`match-status ${message.tone}`} role="status" aria-live="polite">
      <span>{message.text}</span>
      {message.cancel ? (
        <Button size="1" variant="soft" color="gray" onClick={() => queue.cancel()}>
          Cancel
        </Button>
      ) : (
        state.phase === "ended" && (
          <Button size="1" variant="soft" color="gray" onClick={() => queue.dismiss()}>
            Dismiss
          </Button>
        )
      )}
    </div>
  );
}

/** Network match banner: queue search, connecting, waiting, reconnecting,
 * ended, rejoin. A search shows instead of the match banner, since a found
 * match ends the search and searching is offered only outside a match. */
export function MatchStatus() {
  const link = useStore(networkLink, (l) => l);
  const queue = useStore(matchQueue, (q) => q);
  const searching = useStore(queue?.state ?? IDLE, (s) => s.phase !== "idle");
  if (queue && searching) return <QueueStatus queue={queue} />;
  return link ? <LinkStatus link={link} /> : null;
}
