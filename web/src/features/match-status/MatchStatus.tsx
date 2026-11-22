import { useEffect, useState } from "react";
import { Button } from "@radix-ui/themes";
import type { RoundResult } from "../../bridge/snapshot";
import { IDLE_QUEUE, isQueueBarVisible } from "../../domain/matchmaking";
import { matchMessage } from "../../domain/network";
import type { Link } from "../../runtime/network";
import { matchQueue, networkLink } from "../../state/network";
import { engine, scoreboardVisible } from "../../state/session";
import { createStore, useStore } from "../../state/store";

const NO_QUEUE = createStore(IDLE_QUEUE);

function LinkStatus({ link }: { link: Link }) {
  const state = useStore(link.state, (s) => s);
  const scoreboardOpen = useStore(scoreboardVisible, (v) => v);
  const queue = useStore(matchQueue, (q) => q);
  const queueVisible = useStore(queue?.state ?? NO_QUEUE, isQueueBarVisible);
  const joined = useStore(engine, (s) =>
    s?.networked && s.roster ? s.roster.filter((unit) => !unit.disconnected).length : null,
  );
  const seats = useStore(engine, (s) => (s?.networked ? (s.roster?.length ?? null) : null));
  const netResult = useStore(engine, (s) => (s?.networked ? (s.roundResult ?? null) : undefined));
  const statusPhase = state.status?.phase ?? null;
  const [lastResult, setLastResult] = useState<RoundResult | null>(null);
  useEffect(() => {
    if (state.phase === "idle" || state.phase === "connecting") {
      setLastResult(null);
    } else if (netResult !== undefined && statusPhase !== null && statusPhase !== "connecting") {
      if (state.phase === "open") {
        setLastResult(netResult);
      } else if (
        (state.phase === "reconnecting" || state.phase === "ended") &&
        netResult !== null
      ) {
        setLastResult(netResult);
      }
    }
  }, [state.phase, statusPhase, netResult]);
  const effectiveResult =
    state.phase === "ended"
      ? statusPhase !== null && statusPhase !== "connecting"
        ? (netResult ?? lastResult)
        : lastResult
      : lastResult;
  const remembered = state.phase === "idle" || state.phase === "ended" ? link.remembered() : null;
  const message = matchMessage(
    state,
    !!remembered,
    joined !== null && seats !== null ? { joined, seats, seat: state.status?.seat ?? null } : null,
    effectiveResult,
  );
  if (!message || (scoreboardOpen && state.phase === "ended" && state.ended === "MatchOver")) {
    return null;
  }
  return (
    <div
      className={`match-status ${message.tone}${queueVisible ? " below-queue" : ""}`}
      role="status"
      aria-live="polite"
    >
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

/** Network match banner: connecting, waiting, reconnecting, ended, rejoin.
 * Queue searches have their own bar (features/queue). */
export function MatchStatus() {
  const link = useStore(networkLink, (l) => l);
  return link ? <LinkStatus link={link} /> : null;
}
