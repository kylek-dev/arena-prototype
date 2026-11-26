import { Button } from "@radix-ui/themes";
import { useEffect, useRef } from "react";
import { IDLE_QUEUE, readyStatus } from "../../domain/matchmaking";
import type { Queue } from "../../runtime/matchmaking";
import { matchQueue } from "../../state/network";
import { createStore, useStore } from "../../state/store";
import { useClock } from "./useClock";
import "./queue.css";

const NO_QUEUE = createStore(IDLE_QUEUE);

/** While the tab is in the background, the title counts down instead. */
function useTitleAlert(text: string | null) {
  const original = useRef(document.title);
  useEffect(() => {
    if (text === null) return;
    const before = original.current;
    const show = () => {
      document.title = document.hidden ? text : before;
    };
    show();
    document.addEventListener("visibilitychange", show);
    return () => {
      document.removeEventListener("visibilitychange", show);
      document.title = before;
    };
  }, [text]);
}

function Window({ queue }: { queue: Queue }) {
  const state = useStore(queue.state, (s) => s);
  const now = useClock(state.phase === "ready", queue.now, 250);
  const ready = readyStatus(state, now);
  useTitleAlert(ready && !ready.you ? `(${ready.seconds}) Match ready!` : null);
  if (!ready) return null;
  return (
    <div className="match-ready-backdrop">
      <section
        className="match-ready"
        role="alertdialog"
        aria-labelledby="match-ready-title"
        aria-describedby="match-ready-detail"
        data-keyboard-owner="true"
      >
        <h2 id="match-ready-title">{ready.title}</h2>
        <div
          className="match-ready-countdown"
          role="timer"
          aria-label={`${ready.seconds} seconds left`}
        >
          <span className="match-ready-seconds">{ready.seconds}</span>
          <div className="match-ready-track" aria-hidden="true">
            <div className="match-ready-fill" style={{ width: `${ready.fraction * 100}%` }} />
          </div>
        </div>
        <div className="match-ready-players" aria-hidden="true">
          {Array.from({ length: ready.total }, (_, index) => (
            <span
              // Positions only: who accepted is not shown, just how many.
              key={index}
              className={`match-ready-player ${index < ready.accepted ? "accepted" : ""}`}
            />
          ))}
        </div>
        <p id="match-ready-detail" className="match-ready-detail">
          {ready.you
            ? `You have accepted. Waiting for ${ready.total - ready.accepted} more ${ready.total - ready.accepted === 1 ? "player" : "players"} (${ready.progress}).`
            : `${ready.progress}. Accept within ${ready.seconds} seconds to keep your place in the queue.`}
        </p>
        {ready.you ? null : (
          <div className="match-ready-actions">
            <Button size="3" autoFocus onClick={() => queue.accept()}>
              Accept
            </Button>
            <Button size="3" variant="soft" color="gray" onClick={() => queue.decline()}>
              Decline
            </Button>
          </div>
        )}
      </section>
    </div>
  );
}

/** A found match for everyone in it to accept or decline, with the time
 * left and how many have accepted. Shown over everything, wherever the
 * player is. */
export function MatchReady() {
  const queue = useStore(matchQueue, (q) => q);
  const ready = useStore(queue?.state ?? NO_QUEUE, (s) => s.phase === "ready");
  return queue && ready ? <Window queue={queue} /> : null;
}
