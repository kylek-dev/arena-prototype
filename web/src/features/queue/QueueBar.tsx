import { Button } from "@radix-ui/themes";
import { IDLE_QUEUE, endedText, noticeText, queueStatus } from "../../domain/matchmaking";
import type { Queue } from "../../runtime/matchmaking";
import { matchQueue } from "../../state/network";
import { createStore, useStore } from "../../state/store";
import { useClock } from "./useClock";
import "./queue.css";

const NO_QUEUE = createStore(IDLE_QUEUE);
/** Why the party left the queue: worth telling wherever the player is. */
const LEFT_QUEUE = new Set([
  "member_left",
  "declined",
  "not_accepted",
  "member_declined",
  "member_not_accepted",
]);

function Bar({ queue }: { queue: Queue }) {
  const state = useStore(queue.state, (s) => s);
  const now = useClock(state.phase === "queued", queue.now);
  const status = queueStatus(state, now);
  if (state.connection === "ended")
    return (
      <div className="queue-bar warning" role="status">
        <span className="queue-bar-text">{endedText(state.ended)}</span>
        <Button size="1" variant="soft" color="gray" onClick={() => queue.dismiss()}>
          OK
        </Button>
      </div>
    );
  if (status)
    return (
      <div className="queue-bar" role="status" aria-label="Queue">
        <span className="queue-pulse" aria-hidden="true" />
        <div className="queue-bar-body">
          <div className="queue-bar-head">
            <strong>{status.title}</strong>
            <span className="queue-timer" aria-label={`Waiting ${status.waited}`}>
              {status.waited}
            </span>
          </div>
          <div className="queue-bar-detail">
            {status.expected}
            {status.players && ` · ${status.players}`}
          </div>
          {state.notice === "requeued" && (
            <div className="queue-bar-note">{noticeText("requeued")}</div>
          )}
        </div>
        <Button size="1" variant="soft" color="gray" onClick={() => queue.cancel()}>
          Cancel
        </Button>
      </div>
    );
  if (state.phase === "idle" && state.notice && LEFT_QUEUE.has(state.notice))
    return (
      <div className="queue-bar warning" role="status">
        <span className="queue-bar-text">{noticeText(state.notice)}</span>
        <Button size="1" variant="soft" color="gray" onClick={() => queue.dismiss()}>
          OK
        </Button>
      </div>
    );
  return null;
}

/** The queue at a glance: which queue, time waited, expected wait and
 * players queued, with Cancel; or why the search ended. Shown on the home
 * screen and in the arena alike. */
export function QueueBar() {
  const queue = useStore(matchQueue, (q) => q);
  const visible = useStore(queue?.state ?? NO_QUEUE, (s) =>
    s.connection === "ended" ? 1 : s.phase === "queued" ? 2 : s.notice ? 3 : 0,
  );
  return queue && visible ? <Bar queue={queue} /> : null;
}
