import { Button } from "@radix-ui/themes";
import { matchMessage } from "../../domain/network";
import type { Link } from "../../runtime/network";
import { networkLink } from "../../state/network";
import { engine } from "../../state/session";
import { useStore } from "../../state/store";

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

/** Network match banner: connecting, waiting, reconnecting, ended, rejoin. */
export function MatchStatus() {
  const link = useStore(networkLink, (l) => l);
  return link ? <LinkStatus link={link} /> : null;
}
