import { Button } from "@radix-ui/themes";
import {
  IDLE_QUEUE,
  LEFT_QUEUE_NOTICES,
  endedText,
  isQueueBarVisible,
  modeLabel,
  noticeText,
  queueStatus,
} from "../../domain/matchmaking";
import type { LinkState } from "../../domain/network";
import type { Queue } from "../../runtime/matchmaking";
import { customizerVisible } from "../../state/appearance";
import { matchQueue, networkLink } from "../../state/network";
import { customLobbyVisible, scoreboardVisible, setPanel } from "../../state/session";
import { createStore, useStore } from "../../state/store";
import { useClock } from "./useClock";
import "./queue.css";

const NO_QUEUE = createStore(IDLE_QUEUE);
const NO_LINK = createStore<LinkState>({ phase: "idle", attempt: 0, ended: null, status: null });

/** Canonical top-center visibility check shared by QueueBar and HUD elements using `.below-queue`. */
export function useQueueBarVisible(): boolean {
  const queue = useStore(matchQueue, (q) => q);
  const lobbyOpen = useStore(customLobbyVisible, (v) => v);
  const scoreboardOpen = useStore(scoreboardVisible, (v) => v);
  const link = useStore(networkLink, (l) => l);
  const linkPhase = useStore(link?.state ?? NO_LINK, (s) => s.phase);
  const inMatch = linkPhase !== "idle" && linkPhase !== "ended";
  return useStore(queue?.state ?? NO_QUEUE, (state) =>
    isQueueBarVisible(state, { lobbyOpen, inMatch, scoreboardOpen }),
  );
}

function Bar({ queue }: { queue: Queue }) {
  const state = useStore(queue.state, (s) => s);
  const lobbyOpen = useStore(customLobbyVisible, (v) => v);
  const scoreboardOpen = useStore(scoreboardVisible, (v) => v);
  const link = useStore(networkLink, (l) => l);
  const linkPhase = useStore(link?.state ?? NO_LINK, (s) => s.phase);
  const inMatch = linkPhase !== "idle" && linkPhase !== "ended";
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
  if (state.lobby && !lobbyOpen && !inMatch && !scoreboardOpen)
    return (
      <div className="queue-bar" role="status" aria-label="Custom lobby">
        <div className="queue-bar-body">
          <div className="queue-bar-head">
            <strong>
              Custom lobby · {state.lobby.code} ({modeLabel(state.lobby.mode)})
            </strong>
          </div>
          <div className="queue-bar-detail">
            {state.lobby.members.length} {state.lobby.members.length === 1 ? "player" : "players"}{" "}
            in lobby
          </div>
        </div>
        <Button
          size="1"
          variant="soft"
          onClick={() => {
            setPanel(null);
            customizerVisible.set(false);
            customLobbyVisible.set(true);
          }}
        >
          Open lobby
        </Button>
      </div>
    );
  if (state.phase === "idle" && state.notice && LEFT_QUEUE_NOTICES.has(state.notice))
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

/** The queue (or minimized custom lobby) at a glance: shown on the home
 * screen and in the arena alike. */
export function QueueBar() {
  const queue = useStore(matchQueue, (q) => q);
  const visible = useQueueBarVisible();
  return queue && visible ? <Bar queue={queue} /> : null;
}
