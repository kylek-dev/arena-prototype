import { Button, Text } from "@radix-ui/themes";
import { useState } from "react";
import {
  IDLE_QUEUE,
  QUEUE_MODES,
  endedText,
  modeLabel,
  noticeText,
  queueBlocked,
  type QueueMode,
} from "../../domain/matchmaking";
import type { Queue } from "../../runtime/matchmaking";
import { customizerVisible } from "../../state/appearance";
import { matchQueue } from "../../state/network";
import { customLobbyVisible, setPanel } from "../../state/session";
import { AccountPanel } from "../account/AccountPanel";
import { Friends } from "../friends/Friends";
import { createStore, useStore } from "../../state/store";
import { Hint } from "../../ui/Tooltip";
import "./play-online.css";

const NO_QUEUE = createStore(IDLE_QUEUE);
const REFUSALS = new Set([
  "not_logged_in",
  "no_such_friend",
  "offline",
  "unavailable",
  "rate_limited",
  "not_leader",
  "party_too_big",
  "party_full",
  "party_busy",
  "no_such_party",
  "in_lobby",
  "no_such_lobby",
  "lobby_full",
]);

function JoinLobby({ queue }: { queue: Queue }) {
  const [open, setOpen] = useState(false);
  const [code, setCode] = useState("");
  if (!open)
    return (
      <Button size="1" variant="surface" color="gray" onClick={() => setOpen(true)}>
        Join custom lobby
      </Button>
    );
  const join = () => {
    if (queue.joinLobby(code)) {
      setOpen(false);
      setCode("");
    }
  };
  return (
    <div className="party-join">
      <input
        value={code}
        maxLength={12}
        placeholder="Code"
        aria-label="Custom lobby code"
        autoComplete="off"
        spellCheck={false}
        onChange={(event) => setCode(event.target.value.toUpperCase())}
        onKeyDown={(event) => event.key === "Enter" && join()}
      />
      <Button size="1" className="action-button" onClick={join} disabled={!code.trim()}>
        Join
      </Button>
      <Button
        size="1"
        className="action-button"
        variant="surface"
        color="gray"
        onClick={() => setOpen(false)}
      >
        Cancel
      </Button>
    </div>
  );
}

/** Party, public queue and private custom match controls. In game (the
 * launcher's Play window) it is only the queues and custom games: account,
 * friends and party invites stay on the home screen. */
export function PlayOnline({
  onQueue,
  inGame = false,
}: {
  onQueue?: () => void;
  inGame?: boolean;
}) {
  const queue = useStore(matchQueue, (q) => q);
  const state = useStore(queue?.state ?? NO_QUEUE, (s) => s);
  if (!queue) return null;
  const party = state.party?.code ? state.party : null;
  const lobby = state.lobby;
  const searching = state.phase !== "idle";
  // Answers to this panel's own requests; the queue bar tells the rest.
  const notice = REFUSALS.has(state.notice ?? "") ? noticeText(state.notice) : null;
  const start = (mode: QueueMode) => {
    queue.queue(mode);
    onQueue?.();
  };
  const openLobby = () => {
    setPanel(null);
    customizerVisible.set(false);
    customLobbyVisible.set(true);
  };
  return (
    <section className="stack play-online" aria-labelledby="play-online-heading">
      <Text id="play-online-heading" size="2" weight="medium">
        Play online
      </Text>
      {!inGame && <AccountPanel />}
      {!inGame && <Friends />}
      {state.connection === "ended" && (
        <div className="play-online-ended" role="alert">
          <Text size="1">{endedText(state.ended)}</Text>
          <Button size="1" variant="soft" color="gray" onClick={() => queue.dismiss()}>
            OK
          </Button>
        </div>
      )}
      {/* Parties form by inviting friends; their codes stay internal. */}
      {!lobby && party && (
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
              disabled={searching}
              onClick={() => queue.leaveParty()}
            >
              Leave party
            </Button>
          </div>
        </div>
      )}
      {notice && (
        <Text size="1" className="play-online-notice" role="status">
          {notice}
        </Text>
      )}
      {lobby ? (
        <div className="party" role="group" aria-label="Custom match">
          <Text size="1">
            Custom lobby <strong className="party-code">{lobby.code}</strong>,{" "}
            {modeLabel(lobby.mode)}, {lobby.members.length}{" "}
            {lobby.members.length === 1 ? "player" : "players"}.
          </Text>
          <div className="party-actions">
            <Button size="1" onClick={openLobby}>
              Open lobby
            </Button>
            <Button
              size="1"
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
        </div>
      ) : (
        <>
          {searching && state.mode ? (
            <div className="stack">
              <Text size="1" color="gray">
                {state.phase === "ready"
                  ? "A match has been found. Accept it to continue."
                  : `Searching for a ${modeLabel(state.mode)} match.`}
              </Text>
              {state.phase === "queued" && (
                <Button variant="soft" color="gray" onClick={() => queue.cancel()}>
                  Leave queue
                </Button>
              )}
            </div>
          ) : party && !party.leader ? (
            <Text size="1" color="gray">
              The party leader selects the queue.
            </Text>
          ) : (
            <div className="queue-modes">
              {QUEUE_MODES.map((mode) => {
                const blocked = queueBlocked(state, mode.id);
                return (
                  <Button
                    key={mode.id}
                    variant="surface"
                    disabled={!!blocked}
                    title={blocked ?? mode.hint}
                    onClick={() => start(mode.id)}
                  >
                    {mode.label}
                  </Button>
                );
              })}
            </div>
          )}
          <div className="party" role="group" aria-label="Custom match">
            <div className="party-actions">
              <Hint text="Create a private lobby. You choose the teams, team size and round limit.">
                <Button
                  size="1"
                  variant="surface"
                  disabled={searching}
                  onClick={() => queue.createLobby("1v1")}
                >
                  Host custom match
                </Button>
              </Hint>
              {!searching && <JoinLobby queue={queue} />}
            </div>
          </div>
        </>
      )}
    </section>
  );
}
