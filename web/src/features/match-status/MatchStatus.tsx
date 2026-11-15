import { Button } from "@radix-ui/themes";
import { matchMessage } from "../../domain/network";
import type { Link } from "../../runtime/network";
import { networkLink } from "../../state/network";
import { useStore } from "../../state/store";

function LinkStatus({ link }: { link: Link }) {
  const state = useStore(link.state, (s) => s);
  const remembered = state.phase === "idle" ? link.remembered() : null;
  const message = matchMessage(state, !!remembered);
  if (!message) return null;
  return (
    <div className={`match-status ${message.tone}`} role="status" aria-live="polite">
      <span>{message.text}</span>
      {message.action === "rejoin" && remembered && (
        <Button size="1" onClick={() => link.join(remembered)}>
          Rejoin match
        </Button>
      )}
      {message.action === "dismiss" && (
        <Button size="1" variant="soft" color="gray" onClick={() => link.leave()}>
          Dismiss
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
