import { Button, Text } from "@radix-ui/themes";
import { useEffect, useRef, useState } from "react";
import {
  memberLabel,
  memberName,
  ALLOWED_ROUND_SECONDS,
  CUSTOM_MAPS,
  IDLE_QUEUE,
  QUEUE_MODES,
  customInviteLink,
  formatRoundLimit,
  lobbyReadinessText,
  modeLabel,
  noticeText,
  teamSizeForMode,
  type CustomLobby as CustomLobbyData,
  type CustomMapId,
  type LobbyMember,
  type LobbySlot,
  type QueueMode,
} from "../../domain/matchmaking";
import type { LinkState } from "../../domain/network";
import type { Queue } from "../../runtime/matchmaking";
import { customizerVisible } from "../../state/appearance";
import { matchQueue, networkLink } from "../../state/network";
import { customLobbyVisible, error, scoreboardVisible, setPanel } from "../../state/session";
import { createStore, useStore } from "../../state/store";

const NO_QUEUE = createStore(IDLE_QUEUE);
const NO_LINK = createStore<LinkState>({ phase: "idle", attempt: 0, ended: null, status: null });

const LOBBY_NOTICES = new Set([
  "no_such_lobby",
  "lobby_full",
  "not_host",
  "slot_full",
  "lobby_not_full",
  "lobby_too_big",
  "no_such_member",
  "invalid_settings",
  "rate_limited",
  "spectators_disabled",
  "spectators_occupied",
]);

function LobbyInvite({ code }: { code: string }) {
  const link = customInviteLink(window.location, code);
  const [copied, setCopied] = useState<"yes" | "failed" | null>(null);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(link);
      setCopied("yes");
    } catch {
      setCopied("failed");
    }
  };
  return (
    <div className="custom-lobby-invite">
      <div className="custom-lobby-code-row">
        <span className="custom-lobby-code-label">Invite code</span>
        <strong className="custom-lobby-code">{code}</strong>
        <Button size="1" variant="soft" onClick={copy}>
          {copied === "yes" ? "Link copied" : "Copy invite link"}
        </Button>
      </div>
      {copied === "failed" && (
        <input
          className="custom-lobby-link"
          readOnly
          value={link}
          aria-label="Custom lobby invite link"
          onFocus={(event) => event.currentTarget.select()}
        />
      )}
    </div>
  );
}

function MemberRow({
  member,
  label,
  isHost,
  teamAFull,
  teamBFull,
  spectatorsFull,
  queue,
}: {
  member: LobbyMember;
  /** The name buttons use, unique within the lobby. */
  label: string;
  isHost: boolean;
  teamAFull: boolean;
  teamBFull: boolean;
  spectatorsFull: boolean;
  queue: Queue;
}) {
  const otherTeam: LobbySlot = member.slot === "team_a" ? "team_b" : "team_a";
  const otherTeamLabel = otherTeam === "team_a" ? "Team A" : "Team B";
  return (
    <li className={`custom-lobby-member${member.you ? " you" : ""}`}>
      <div className="custom-lobby-member-identity">
        <span className="custom-lobby-member-name">{memberName(member)}</span>
        {member.you && <span className="custom-lobby-badge you">You</span>}
        {member.host && <span className="custom-lobby-badge host">Host</span>}
      </div>
      {isHost && (
        <div className="custom-lobby-member-actions">
          {member.slot === "spectator" ? (
            <>
              <Button
                className="action-button"
                size="1"
                variant="surface"
                color="gray"
                disabled={teamAFull}
                aria-label={`Move ${label} to Team A`}
                onClick={() => queue.moveLobbyMember(member.id, "team_a")}
              >
                Team A
              </Button>
              <Button
                className="action-button"
                size="1"
                variant="surface"
                color="gray"
                disabled={teamBFull}
                aria-label={`Move ${label} to Team B`}
                onClick={() => queue.moveLobbyMember(member.id, "team_b")}
              >
                Team B
              </Button>
            </>
          ) : (
            <>
              <Button
                className="action-button"
                size="1"
                variant="surface"
                color="gray"
                aria-label={`Move ${label} to ${otherTeamLabel}`}
                onClick={() => queue.moveLobbyMember(member.id, otherTeam)}
              >
                {otherTeamLabel}
              </Button>
              <Button
                className="action-button"
                size="1"
                variant="surface"
                color="gray"
                disabled={spectatorsFull}
                aria-label={`Move ${label} to Spectators`}
                onClick={() => queue.moveLobbyMember(member.id, "spectator")}
              >
                Spectate
              </Button>
            </>
          )}
          {!member.you && (
            <>
              <Button
                className="action-button"
                size="1"
                variant="surface"
                color="gray"
                aria-label={`Make ${label} host`}
                onClick={() => queue.transferLobbyHost(member.id)}
              >
                Make host
              </Button>
              <Button
                className="action-button"
                size="1"
                variant="surface"
                color="red"
                aria-label={`Remove ${label}`}
                onClick={() => queue.kickLobbyMember(member.id)}
              >
                Remove
              </Button>
            </>
          )}
        </div>
      )}
    </li>
  );
}

function SlotColumn({
  title,
  slot,
  tone,
  capacity,
  members,
  teamAFull,
  teamBFull,
  spectatorsFull,
  lobby,
  queue,
}: {
  title: string;
  slot: LobbySlot;
  tone: "team-a" | "team-b" | "spectator";
  capacity: number;
  members: readonly LobbyMember[];
  teamAFull: boolean;
  teamBFull: boolean;
  spectatorsFull: boolean;
  lobby: CustomLobbyData;
  queue: Queue;
}) {
  const me = lobby.members.find((m) => m.you);
  const inSlot = me?.slot === slot;
  const full = members.length >= capacity;
  const hostCanSwap =
    lobby.host &&
    me &&
    ((me.slot === "team_a" && slot === "team_b") || (me.slot === "team_b" && slot === "team_a"));
  const spectateDisabled = slot === "spectator" && !lobby.settings.allowSpectators;
  const canJoin = !inSlot && !spectateDisabled && (!full || hostCanSwap);
  const joinLabel =
    slot === "spectator"
      ? spectateDisabled
        ? "Spectators off"
        : "Join spectators"
      : full && hostCanSwap
        ? `Swap to ${title}`
        : `Join ${title}`;

  return (
    <section className={`custom-lobby-column ${tone}`} aria-label={title}>
      <header className="custom-lobby-column-head">
        <h3>{title}</h3>
        <span className="custom-lobby-capacity">
          {members.length} / {capacity}
        </span>
      </header>
      <ul className="custom-lobby-roster">
        {members.length === 0 ? (
          <li className="custom-lobby-empty">Open slot</li>
        ) : (
          members.map((member) => (
            <MemberRow
              key={member.id}
              member={member}
              label={memberLabel(member, lobby.members)}
              isHost={lobby.host}
              teamAFull={teamAFull}
              teamBFull={teamBFull}
              spectatorsFull={spectatorsFull}
              queue={queue}
            />
          ))
        )}
      </ul>
      {!inSlot && (
        <Button
          size="1"
          variant="soft"
          color="gray"
          disabled={!canJoin}
          onClick={() => queue.setLobbySlot(slot)}
        >
          {joinLabel}
        </Button>
      )}
    </section>
  );
}

/** Private custom match lobby modal: invite code, team/spectator slots,
 * host controls, and server-enforced versioned settings. */
export function CustomLobby() {
  const queue = useStore(matchQueue, (q) => q);
  const state = useStore(queue?.state ?? NO_QUEUE, (s) => s);
  const link = useStore(networkLink, (l) => l);
  const linkPhase = useStore(link?.state ?? NO_LINK, (s) => s.phase);
  const inMatch = linkPhase !== "idle" && linkPhase !== "ended";
  const scoreboardOpen = useStore(scoreboardVisible, (v) => v);
  const visible = useStore(customLobbyVisible, (v) => v);
  const fatalError = useStore(error, Boolean);
  const previousCode = useRef<string | null>(null);
  const [starting, setStarting] = useState(false);

  useEffect(() => {
    const code = state.lobby?.code ?? null;
    if (code && code !== previousCode.current) {
      if (!inMatch && !scoreboardOpen) {
        setPanel(null);
        customizerVisible.set(false);
        customLobbyVisible.set(true);
      }
    } else if (!code && previousCode.current) {
      customLobbyVisible.set(false);
    }
    previousCode.current = code;
  }, [state.lobby?.code, inMatch, scoreboardOpen]);

  useEffect(() => {
    if (
      !visible ||
      !state.lobby?.canStart ||
      state.notice !== null ||
      state.connection !== "online"
    ) {
      setStarting(false);
    }
  }, [visible, state.lobby?.canStart, state.notice, state.connection]);

  if (!queue || !state.lobby || !visible || fatalError) return null;

  const lobby = state.lobby;
  const teamSize = teamSizeForMode(lobby.mode);
  const teamA = lobby.members.filter((m) => m.slot === "team_a");
  const teamB = lobby.members.filter((m) => m.slot === "team_b");
  const spectators = lobby.members.filter((m) => m.slot === "spectator");
  const teamAFull = teamA.length >= teamSize;
  const teamBFull = teamB.length >= teamSize;
  const spectatorsFull =
    !lobby.settings.allowSpectators || spectators.length >= lobby.spectatorSlots;
  const notice = LOBBY_NOTICES.has(state.notice ?? "") ? noticeText(state.notice) : null;
  const readiness = lobbyReadinessText(lobby);

  return (
    <div className="custom-lobby-backdrop">
      <section
        className="custom-lobby"
        role="dialog"
        aria-modal="true"
        aria-labelledby="custom-lobby-title"
        aria-describedby="custom-lobby-status"
        data-keyboard-owner="true"
      >
        <header className="custom-lobby-header">
          <div className="custom-lobby-heading">
            <h2 id="custom-lobby-title">Custom match · {modeLabel(lobby.mode)}</h2>
            <Text size="1" color="gray">
              Invite friends by code or link. Player and spectator tickets are issued when the host
              starts.
            </Text>
          </div>
          <div className="custom-lobby-header-actions">
            <Button
              size="1"
              variant="soft"
              color="gray"
              onClick={() => customLobbyVisible.set(false)}
            >
              Minimize
            </Button>
            <Button
              size="1"
              variant="soft"
              color="red"
              onClick={() => {
                customLobbyVisible.set(false);
                queue.leaveLobby();
              }}
            >
              Leave lobby
            </Button>
          </div>
        </header>

        <LobbyInvite key={lobby.code} code={lobby.code} />

        <div className="custom-lobby-settings" role="group" aria-label="Match settings">
          <label className="custom-lobby-field">
            <span>Size</span>
            <select
              aria-label="Match size"
              value={lobby.mode}
              disabled={!lobby.host}
              onChange={(event) => queue.setLobbyMode(event.target.value as QueueMode)}
            >
              {QUEUE_MODES.map((mode) => (
                <option key={mode.id} value={mode.id}>
                  {mode.label}
                </option>
              ))}
            </select>
          </label>
          <label className="custom-lobby-field">
            <span>Map</span>
            <select
              aria-label="Arena map"
              value={lobby.settings.map}
              disabled={!lobby.host}
              onChange={(event) =>
                queue.setLobbySettings({
                  ...lobby.settings,
                  map: event.target.value as CustomMapId,
                })
              }
            >
              {CUSTOM_MAPS.map((map) => (
                <option key={map.id} value={map.id}>
                  {map.label}
                </option>
              ))}
            </select>
          </label>
          <label className="custom-lobby-field">
            <span>Round limit</span>
            <select
              aria-label="Round time limit"
              value={lobby.settings.roundSeconds}
              disabled={!lobby.host}
              onChange={(event) =>
                queue.setLobbySettings({
                  ...lobby.settings,
                  roundSeconds: Number(event.target.value) as typeof lobby.settings.roundSeconds,
                })
              }
            >
              {ALLOWED_ROUND_SECONDS.map((seconds) => (
                <option key={seconds} value={seconds}>
                  {formatRoundLimit(seconds)}
                </option>
              ))}
            </select>
          </label>
          <label className="custom-lobby-field">
            <span>Spectators</span>
            <select
              aria-label="Allow spectators"
              value={lobby.settings.allowSpectators ? "allowed" : "off"}
              disabled={!lobby.host}
              onChange={(event) =>
                queue.setLobbySettings({
                  ...lobby.settings,
                  allowSpectators: event.target.value === "allowed",
                })
              }
            >
              <option value="allowed">Allowed</option>
              <option value="off">Off</option>
            </select>
          </label>
        </div>

        <div className="custom-lobby-columns">
          <SlotColumn
            title="Team A"
            slot="team_a"
            tone="team-a"
            capacity={teamSize}
            members={teamA}
            teamAFull={teamAFull}
            teamBFull={teamBFull}
            spectatorsFull={spectatorsFull}
            lobby={lobby}
            queue={queue}
          />
          <SlotColumn
            title="Team B"
            slot="team_b"
            tone="team-b"
            capacity={teamSize}
            members={teamB}
            teamAFull={teamAFull}
            teamBFull={teamBFull}
            spectatorsFull={spectatorsFull}
            lobby={lobby}
            queue={queue}
          />
          <SlotColumn
            title="Spectators"
            slot="spectator"
            tone="spectator"
            capacity={lobby.spectatorSlots}
            members={spectators}
            teamAFull={teamAFull}
            teamBFull={teamBFull}
            spectatorsFull={spectatorsFull}
            lobby={lobby}
            queue={queue}
          />
        </div>

        {notice && (
          <p className="custom-lobby-notice" role="status">
            {notice}
          </p>
        )}

        <footer className="custom-lobby-footer">
          <p id="custom-lobby-status" className="custom-lobby-status">
            {readiness}
          </p>
          {lobby.host && (
            <Button
              size="2"
              disabled={!lobby.canStart || starting}
              title={lobby.canStart ? undefined : readiness}
              onClick={() => {
                setStarting(true);
                queue.startLobby();
              }}
            >
              {starting ? "Starting…" : "Start match"}
            </Button>
          )}
        </footer>
      </section>
    </div>
  );
}
