import { inMatch as inMatchOf } from "../../domain/network";
import { Button, Tabs } from "@radix-ui/themes";
import { useState } from "react";
import {
  IDLE_QUEUE,
  QUEUE_MODES,
  modeLabel,
  noticeText,
  normalizeCode,
  queueBlocked,
} from "../../domain/matchmaking";
import type { LinkState } from "../../domain/network";
import { dispatch, input } from "../../runtime/commands";
import type { Queue } from "../../runtime/matchmaking";
import { customizerVisible } from "../../state/appearance";
import { matchQueue, networkLink } from "../../state/network";
import { customLobbyVisible, engine, setPanel } from "../../state/session";
import { createStore, useStore } from "../../state/store";
import { playTab, playWindowVisible, type PlayTab } from "../../state/windows";
import { DockWindow } from "../../ui/DockWindow";
import { Hint } from "../../ui/Tooltip";
import { OUT_OF_HUB } from "../../domain/hub";
import { enterHub, leaveHub } from "../../runtime/hub";

const NO_LINK = createStore<LinkState>({ phase: "idle", attempt: 0, ended: null, status: null });
const NO_QUEUE = createStore(IDLE_QUEUE);
const NO_HUB = createStore(OUT_OF_HUB);

const TABS: readonly { id: PlayTab; label: string; hint: string }[] = [
  { id: "practice", label: "Practice", hint: "Practice against stationary opponents." },
  { id: "queue", label: "Queue", hint: "Join a public queue for 1v1, 2v2 or 3v3." },
  { id: "host", label: "Host", hint: "Host or join a private custom match." },
];
/** Refusals answered in this window; the queue bar reports the rest. */
const REFUSALS = new Set([
  "not_leader",
  "party_too_big",
  "party_busy",
  "in_lobby",
  "no_such_lobby",
  "lobby_full",
  "rate_limited",
]);

function Practice() {
  const size = useStore(engine, (s) => s?.teamSize ?? 1);
  const queue = useStore(matchQueue, (q) => q);
  const hub = useStore(queue?.hub ?? NO_HUB, (h) => h.status);
  if (hub !== "out")
    return (
      <div className="play-tab stack">
        <p className="play-status">
          {hub === "in" ? "You are in the hub." : "Entering the hub."} Practice takes you to the
          arena against stationary opponents.
        </p>
        <Hint text="Leave the hub and start a practice round in the arena.">
          <Button
            variant="soft"
            onClick={() => {
              leaveHub();
              dispatch("reset");
            }}
          >
            Practice in the arena
          </Button>
        </Hint>
      </div>
    );
  return (
    <div className="play-tab stack">
      {queue && (
        <Hint text="Return to the hub, where the other players are.">
          <Button onClick={() => enterHub()}>Return to the hub</Button>
        </Hint>
      )}
      <label className="play-field">
        <span>Practice mode</span>
        <select
          value={size}
          onChange={(event) => {
            const next = Number(event.target.value);
            dispatch("reset");
            input.teamSize = next;
          }}
        >
          <option value="1">1v1</option>
          <option value="2">2v2</option>
          <option value="3">3v3</option>
        </select>
      </label>
      <Hint text="Start a new practice round with the selected team size.">
        <Button variant="soft" color="gray" onClick={() => dispatch("reset")}>
          Reset practice
        </Button>
      </Hint>
    </div>
  );
}

function QueueTab({ queue }: { queue: Queue }) {
  const state = useStore(queue.state, (s) => s);
  const party = state.party?.code ? state.party : null;
  const searching = state.phase !== "idle";
  const notice = REFUSALS.has(state.notice ?? "") ? noticeText(state.notice) : null;
  return (
    <div className="play-tab stack">
      {party && (
        <div className="play-row" role="group" aria-label="Party">
          <span>
            You are in a party of {party.members}.{" "}
            {party.leader ? "You lead the party." : "Another member leads the party."}
          </span>
          <Button
            size="1"
            variant="soft"
            color="gray"
            disabled={searching}
            onClick={() => queue.leaveParty()}
          >
            Leave party
          </Button>
        </div>
      )}
      {state.lobby ? (
        <p className="play-status">
          Public queues are unavailable while you are in a custom lobby.
        </p>
      ) : searching && state.mode ? (
        <>
          <p className="play-status" role="status">
            {state.phase === "ready"
              ? "A match has been found. Accept it to continue."
              : `Searching for a ${modeLabel(state.mode)} match.`}
          </p>
          {state.phase === "queued" && (
            <Button variant="soft" color="gray" onClick={() => queue.cancel()}>
              Leave queue
            </Button>
          )}
        </>
      ) : party && !party.leader ? (
        <p className="play-status">The party leader selects the queue.</p>
      ) : (
        <div className="play-queue-modes">
          {QUEUE_MODES.map((mode) => {
            const blocked = queueBlocked(state, mode.id);
            return (
              <Hint key={mode.id} text={blocked ?? mode.hint}>
                <Button
                  size="3"
                  variant="surface"
                  aria-disabled={blocked ? true : undefined}
                  onClick={() => {
                    if (!blocked) queue.queue(mode.id);
                  }}
                >
                  {mode.label}
                </Button>
              </Hint>
            );
          })}
        </div>
      )}
      {notice && (
        <p className="play-notice" role="status">
          {notice}
        </p>
      )}
    </div>
  );
}

function HostTab({ queue }: { queue: Queue }) {
  const state = useStore(queue.state, (s) => s);
  const [code, setCode] = useState("");
  const lobby = state.lobby;
  const searching = state.phase !== "idle";
  const notice = REFUSALS.has(state.notice ?? "") ? noticeText(state.notice) : null;
  const join = () => {
    if (normalizeCode(code) && queue.joinLobby(code)) setCode("");
  };
  return (
    <div className="play-tab stack">
      {lobby ? (
        <>
          <p className="play-status">
            Custom lobby <strong className="party-code">{lobby.code}</strong>,{" "}
            {modeLabel(lobby.mode)}, {lobby.members.length}{" "}
            {lobby.members.length === 1 ? "player" : "players"}.
          </p>
          <div className="party-actions">
            <Button
              onClick={() => {
                setPanel(null);
                customizerVisible.set(false);
                customLobbyVisible.set(true);
              }}
            >
              Open lobby
            </Button>
            <Button
              variant="soft"
              color="gray"
              onClick={() => {
                customLobbyVisible.set(false);
                queue.leaveLobby();
              }}
            >
              Leave lobby
            </Button>
          </div>
        </>
      ) : (
        <>
          <Hint text="Create a private lobby. You choose the teams, team size and round limit.">
            <Button disabled={searching} onClick={() => queue.createLobby("1v1")}>
              Host custom match
            </Button>
          </Hint>
          <label className="play-field">
            <span>Lobby code</span>
            <span className="play-join">
              <input
                value={code}
                maxLength={12}
                placeholder="ABC123"
                aria-label="Custom lobby code"
                autoComplete="off"
                spellCheck={false}
                disabled={searching}
                onChange={(event) => setCode(event.target.value.toUpperCase())}
                onKeyDown={(event) => event.key === "Enter" && join()}
              />
              <Button variant="surface" disabled={searching || !normalizeCode(code)} onClick={join}>
                Join
              </Button>
            </span>
          </label>
        </>
      )}
      {notice && (
        <p className="play-notice" role="status">
          {notice}
        </p>
      )}
    </div>
  );
}

/** P: Play, with Practice, Queue and Host tabs along the bottom. */
export function PlayWindow() {
  const tab = useStore(playTab, (t) => t);
  const queue = useStore(matchQueue, (q) => q);
  const link = useStore(networkLink, (l) => l);
  const inMatch = useStore(link?.state ?? NO_LINK, inMatchOf);
  useStore(queue?.state ?? NO_QUEUE, (s) => s.phase);

  return (
    // Clicking tabs and buttons leaves the keys with the game; only its text
    // field (the lobby code) takes them, as every text field does.
    <DockWindow
      title="Play"
      keyboardOwner={false}
      close={() => playWindowVisible.set(false)}
      className="play-window"
    >
      <Tabs.Root
        className="play-tabs"
        value={tab}
        onValueChange={(value) => playTab.set(value as PlayTab)}
      >
        <Tabs.List aria-label="Play" size="2">
          {TABS.map((entry) => (
            // The tooltip sits inside the trigger: wrapping the trigger would
            // replace its data-state, which draws the active tab's style.
            <Tabs.Trigger key={entry.id} value={entry.id}>
              <Hint text={entry.hint}>
                <span>{entry.label}</span>
              </Hint>
            </Tabs.Trigger>
          ))}
        </Tabs.List>
        <Tabs.Content value={tab} className="play-tab-panel">
          {inMatch ? (
            <p className="play-status">These options are available outside a match.</p>
          ) : tab === "practice" ? (
            <Practice />
          ) : !queue ? (
            <p className="play-status">Online play is unavailable.</p>
          ) : tab === "queue" ? (
            <QueueTab queue={queue} />
          ) : (
            <HostTab queue={queue} />
          )}
        </Tabs.Content>
      </Tabs.Root>
    </DockWindow>
  );
}
