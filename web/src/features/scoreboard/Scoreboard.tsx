import { AdminTag } from "../../ui/AdminTag";
import { Button } from "@radix-ui/themes";
import { useEffect, useState } from "react";
import { IDLE_QUEUE, modeLabel, noticeText, queueBlocked } from "../../domain/matchmaking";
import { useQueueBarVisible } from "../queue/QueueBar";
import type { LinkState } from "../../domain/network";
import {
  DEFAULT_SCOREBOARD_SORT,
  formatMatchDuration,
  SCOREBOARD_COLUMNS,
  scoreboardQueueMode,
  scoreboardTone,
  sortScoreboardRows,
  toggleScoreboardSort,
  type MatchScoreboard,
  type ScoreboardRow,
  type ScoreboardSort,
  type TeamTotals,
} from "../../domain/scoreboard";
import { returnToHub } from "../../runtime/hub";
import { matchQueue, networkLink } from "../../state/network";
import {
  dismissPostMatchScoreboard,
  postMatchScoreboard,
  syncPostMatchScoreboard,
} from "../../state/scoreboard";
import { customLobbyVisible, engine, error } from "../../state/session";
import { createStore, useStore } from "../../state/store";

const NO_QUEUE = createStore(IDLE_QUEUE);
const NO_LINK = createStore<LinkState>({ phase: "idle", attempt: 0, ended: null, status: null });

function TeamRows({
  title,
  tone,
  rows,
  totals,
  sort,
}: {
  title: string;
  tone: "allies" | "opponents";
  rows: readonly ScoreboardRow[];
  totals: TeamTotals;
  sort: ScoreboardSort;
}) {
  const sorted = sortScoreboardRows(rows, sort);
  return (
    <tbody className={`scoreboard-team ${tone}`} aria-label={title}>
      <tr className="scoreboard-team-heading">
        <th scope="colgroup" colSpan={SCOREBOARD_COLUMNS.length + 1}>
          {title}
        </th>
      </tr>
      {sorted.map((row) => (
        <tr
          key={row.actor}
          className={`scoreboard-row${row.isLocalPlayer ? " local" : ""}${row.disconnected ? " disconnected" : ""}`}
        >
          <th scope="row" className="scoreboard-player">
            <span className="scoreboard-player-name">{row.name}</span>
            {row.admin && <AdminTag />}
            {row.isLocalPlayer && <span className="scoreboard-badge you">You</span>}
            {row.disconnected && <span className="scoreboard-badge dc">Disconnected</span>}
          </th>
          {SCOREBOARD_COLUMNS.map((column) => (
            <td
              key={column.key}
              className={`scoreboard-stat${sort.key === column.key ? " sorted" : ""}`}
            >
              {row[column.key]}
            </td>
          ))}
        </tr>
      ))}
      <tr className="scoreboard-totals">
        <th scope="row" className="scoreboard-player">
          {title} total
        </th>
        {SCOREBOARD_COLUMNS.map((column) => (
          <td
            key={column.key}
            className={`scoreboard-stat${sort.key === column.key ? " sorted" : ""}`}
          >
            {totals[column.key]}
          </td>
        ))}
      </tr>
    </tbody>
  );
}

function ScoreboardModal({
  board,
  linkState,
  queueVisible,
  onLeave,
  onQueueAgain,
  queueAction,
}: {
  board: MatchScoreboard;
  linkState: LinkState;
  queueVisible: boolean;
  onLeave: () => void;
  onQueueAgain: () => void;
  queueAction: {
    visible: boolean;
    disabled: boolean;
    label: string;
    hint: string | null;
  };
}) {
  const [sort, setSort] = useState<ScoreboardSort>(DEFAULT_SCOREBOARD_SORT);
  const tone = scoreboardTone(board.result);
  const mode = scoreboardQueueMode(board);
  const duration = formatMatchDuration(board.durationTicks, board.tickRate);
  const statusDetail =
    linkState.phase === "open" && linkState.status?.phase === "finished"
      ? "Match closing shortly"
      : "Match ended";

  return (
    <div className={`scoreboard-backdrop${queueVisible ? " below-queue" : ""}`}>
      <section
        className={`scoreboard ${tone}`}
        role="dialog"
        aria-modal="true"
        aria-labelledby="scoreboard-title"
        aria-describedby="scoreboard-meta"
        data-keyboard-owner="true"
      >
        <header className="scoreboard-header">
          <div className="scoreboard-heading">
            <h2 id="scoreboard-title" className="scoreboard-title">
              {board.resultLabel}
            </h2>
            <p id="scoreboard-meta" className="scoreboard-meta">
              {mode ? `${modeLabel(mode)} · ` : ""}
              Duration {duration} · {statusDetail}
            </p>
          </div>
        </header>

        <div className="scoreboard-table-wrap">
          <table className="scoreboard-table">
            <thead>
              <tr>
                <th scope="col" className="scoreboard-player-col">
                  Player
                </th>
                {SCOREBOARD_COLUMNS.map((column) => {
                  const active = sort.key === column.key;
                  const ariaSort = active
                    ? sort.direction === "desc"
                      ? "descending"
                      : "ascending"
                    : "none";
                  return (
                    <th
                      key={column.key}
                      scope="col"
                      className={`scoreboard-stat-col${active ? " active" : ""}`}
                      aria-sort={ariaSort}
                    >
                      <button
                        type="button"
                        className="scoreboard-sort-button"
                        aria-label={`Sort by ${column.label}`}
                        onClick={() =>
                          setSort((current) => toggleScoreboardSort(current, column.key))
                        }
                      >
                        <span className="scoreboard-col-full">{column.label}</span>
                        <span className="scoreboard-col-short" aria-hidden="true">
                          {column.shortLabel}
                        </span>
                        <span className="scoreboard-sort-indicator" aria-hidden="true">
                          {active ? (sort.direction === "desc" ? "↓" : "↑") : ""}
                        </span>
                      </button>
                    </th>
                  );
                })}
              </tr>
            </thead>
            <TeamRows
              title={board.spectating ? "Team A" : "Your team"}
              tone="allies"
              rows={board.allies}
              totals={board.allyTotals}
              sort={sort}
            />
            <TeamRows
              title={board.spectating ? "Team B" : "Opponents"}
              tone="opponents"
              rows={board.opponents}
              totals={board.opponentTotals}
              sort={sort}
            />
          </table>
        </div>

        <footer className="scoreboard-footer">
          {queueAction.hint && (
            <p className="scoreboard-hint" role="status">
              {queueAction.hint}
            </p>
          )}
          <div className="scoreboard-actions">
            <Button size="2" variant="soft" color="gray" onClick={onLeave}>
              Leave
            </Button>
            {queueAction.visible && (
              <Button
                size="2"
                autoFocus={!queueAction.disabled}
                disabled={queueAction.disabled}
                title={queueAction.hint ?? undefined}
                onClick={onQueueAgain}
              >
                {queueAction.label}
              </Button>
            )}
          </div>
        </footer>
      </section>
    </div>
  );
}

/** Sortable end-of-match scoreboard modal retained locally after MatchOver. */
export function Scoreboard() {
  const board = useStore(postMatchScoreboard, (b) => b);
  const snap = useStore(engine, (s) => s);
  const fatalError = useStore(error, Boolean);
  const link = useStore(networkLink, (l) => l);
  const linkState = useStore(link?.state ?? NO_LINK, (s) => s);
  const queue = useStore(matchQueue, (q) => q);
  const queueState = useStore(queue?.state ?? NO_QUEUE, (s) => s);
  const queueVisible = useQueueBarVisible();
  const [queuePending, setQueuePending] = useState(false);

  useEffect(() => {
    syncPostMatchScoreboard(snap, link ? linkState : null);
  }, [snap, link, linkState]);

  useEffect(() => {
    if (
      queueState.phase !== "idle" ||
      queueState.notice !== null ||
      queueState.connection === "ended"
    ) {
      setQueuePending(false);
    }
  }, [queueState.phase, queueState.notice, queueState.connection]);

  useEffect(() => {
    if (!board) setQueuePending(false);
  }, [board]);

  if (!board || fatalError) return null;

  const inCustomLobby = queueState.lobby !== null;
  const targetMode = scoreboardQueueMode(board) ?? queueState.mode;
  const blocked = targetMode ? queueBlocked(queueState, targetMode) : null;
  const searching = queueState.phase === "queued" || queueState.phase === "ready";
  const connectingQueue = queuePending || queueState.connection === "connecting";
  const refusalNotice = queueState.phase === "idle" ? noticeText(queueState.notice) : null;
  const queueLabel = inCustomLobby
    ? "Return to lobby"
    : queueState.phase === "ready"
      ? "Match ready"
      : queueState.phase === "queued" && queueState.mode
        ? `In ${modeLabel(queueState.mode)} queue`
        : connectingQueue
          ? "Queueing…"
          : "Queue again";
  const queueHint = inCustomLobby
    ? `Your custom lobby (${queueState.lobby?.code}) is still open for a rematch.`
    : queueState.phase === "ready"
      ? "Your next match is ready."
      : queueState.phase === "queued" && queueState.mode
        ? `Searching for a ${modeLabel(queueState.mode)} match…`
        : (refusalNotice ?? blocked);

  const onLeave = () => {
    dismissPostMatchScoreboard();
    if (link && linkState.phase !== "idle") link.leave();
    returnToHub();
  };

  const onQueueAgain = () => {
    if (inCustomLobby) {
      dismissPostMatchScoreboard();
      if (link && linkState.phase !== "idle") link.leave();
      customLobbyVisible.set(true);
      return;
    }
    if (!queue || !targetMode || blocked || searching || connectingQueue) return;
    setQueuePending(true);
    if (link && (linkState.phase === "ended" || linkState.status?.phase === "finished")) {
      link.leave();
    }
    queue.queue(targetMode);
  };

  return (
    <ScoreboardModal
      board={board}
      linkState={linkState}
      queueVisible={queueVisible}
      onLeave={onLeave}
      onQueueAgain={onQueueAgain}
      queueAction={{
        visible: inCustomLobby || (!board.spectating && queue !== null && targetMode !== null),
        disabled: !inCustomLobby && Boolean(blocked || searching || connectingQueue),
        label: queueLabel,
        hint: inCustomLobby || !board.spectating ? queueHint : null,
      }}
    />
  );
}
