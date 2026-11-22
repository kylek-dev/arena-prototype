import { Button } from "@radix-ui/themes";
import { IDLE_QUEUE, isQueueBarVisible } from "../../domain/matchmaking";
import { matchMessage, type LinkState } from "../../domain/network";
import {
  roundClockFromSeconds,
  roundResultBanner,
  snapshotRemainingSeconds,
} from "../../domain/round-clock";
import { dispatch } from "../../runtime/commands";
import { matchQueue, networkLink } from "../../state/network";
import { engine, error, interaction, scoreboardVisible } from "../../state/session";
import { createStore, useStore } from "../../state/store";

const NO_QUEUE = createStore(IDLE_QUEUE);
const NO_LINK = createStore<LinkState>({ phase: "idle", attempt: 0, ended: null, status: null });

/** Authoritative round countdown timer and round outcome banner.
 * Uses only engine-reported ticks, round limit, tick rate, and result (never a wall clock). */
export function RoundClock() {
  const remainingSeconds = useStore(engine, snapshotRemainingSeconds);
  const won = useStore(engine, (s) => s?.won ?? false);
  const roundResult = useStore(engine, (s) => s?.roundResult ?? null);
  const networked = useStore(engine, (s) => Boolean(s?.networked));
  const suppressed = useStore(
    interaction,
    (m) => m.layout || m.quick || !!m.capture || m.panel === "menu" || m.panel === "settings",
  );
  const scoreboardOpen = useStore(scoreboardVisible, (v) => v);
  const hasScoreboard = useStore(engine, (s) => Boolean(s?.networked && s.won && s.scoreboard));
  const failed = useStore(error, Boolean);
  const queue = useStore(matchQueue, (q) => q);
  const queueVisible = useStore(queue?.state ?? NO_QUEUE, isQueueBarVisible);
  const link = useStore(networkLink, (l) => l);
  const linkState = useStore(link?.state ?? NO_LINK, (s) => s);

  if (suppressed || scoreboardOpen || failed) return null;

  const remembered =
    link && (linkState.phase === "idle" || linkState.phase === "ended") ? link.remembered() : null;
  const matchBannerVisible = link !== null && matchMessage(linkState, Boolean(remembered)) !== null;

  const clock =
    remainingSeconds !== null ? roundClockFromSeconds(remainingSeconds, won, roundResult) : null;
  const banner =
    matchBannerVisible || hasScoreboard ? null : roundResultBanner(roundResult, networked);
  if (!clock && !banner) return null;

  const queueOffset = queueVisible ? " below-queue" : "";

  return (
    <>
      {clock && (
        <div
          className={`round-clock${clock.warning ? " warning" : ""}${clock.expired ? " expired" : ""}${queueOffset}`}
          role="timer"
          aria-label="Round time remaining"
          aria-live="off"
        >
          <span className="round-clock-time">{clock.text}</span>
        </div>
      )}
      {banner && (
        <div
          className={`round-result ${banner.tone}${queueOffset}`}
          role="status"
          aria-live="polite"
        >
          <strong className="round-result-label">{banner.label}</strong>
          <span className="round-result-detail">{banner.detail}</span>
          {!networked && (
            <Button size="1" variant="soft" color="gray" onClick={() => dispatch("reset")}>
              Reset practice
            </Button>
          )}
        </div>
      )}
    </>
  );
}
