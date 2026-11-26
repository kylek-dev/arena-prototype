import { Button } from "@radix-ui/themes";
import type { RosterEntry } from "../../bridge/snapshot";
import { cycleSpectatorCamera, setSpectatorCamera } from "../../runtime/commands";
import { returnToLauncher } from "../../state/appearance";
import { matchQueue, networkLink } from "../../state/network";
import {
  customLobbyVisible,
  engine,
  error,
  interaction,
  scoreboardVisible,
} from "../../state/session";
import { useStore } from "../../state/store";
import { UnitFrame } from "../unit-frames/UnitFrame";

function SpectatorTeamColumn({
  title,
  tone,
  units,
  followActor,
}: {
  title: string;
  tone: "team-a" | "team-b";
  units: readonly RosterEntry[];
  followActor: number | null;
}) {
  return (
    <section className={`spectator-team ${tone}`} aria-label={title}>
      <header className="spectator-team-head">{title}</header>
      <div className="spectator-team-list">
        {units.map((unit) => {
          const active = followActor === unit.actor;
          return (
            <button
              key={unit.actor}
              type="button"
              className={`spectator-unit-entry${active ? " following" : ""}`}
              aria-label={`Follow ${unit.name}`}
              aria-pressed={active}
              onClick={() => setSpectatorCamera(unit.actor)}
            >
              <UnitFrame role={tone === "team-a" ? "teammate" : "opponent"} unit={unit} />
            </button>
          );
        })}
      </div>
    </section>
  );
}

/** Watch-only spectator HUD: both teams' unit frames and camera controls, with no action bars or targeting. */
export function SpectatorHud() {
  const snap = useStore(engine, (s) => s);
  const mode = useStore(interaction, (m) => m);
  const scoreboardOpen = useStore(scoreboardVisible, (v) => v);
  const fatalError = useStore(error, Boolean);
  const link = useStore(networkLink, (l) => l);
  const queue = useStore(matchQueue, (q) => q);

  if (
    !snap?.spectating ||
    fatalError ||
    scoreboardOpen ||
    mode.panel === "menu" ||
    mode.panel === "settings"
  ) {
    return null;
  }

  const roster = snap.roster ?? [];
  const teamA = roster.filter((u) => u.actor % 2 === 0);
  const teamB = roster.filter((u) => u.actor % 2 === 1);
  const followActor = snap.spectatorFollow ?? null;
  const followedUnit =
    followActor !== null ? (roster.find((u) => u.actor === followActor) ?? null) : null;

  const onLeave = () => {
    const hasLobby = Boolean(queue?.state.get().lobby);
    if (link) link.leave();
    if (hasLobby) customLobbyVisible.set(true);
    else returnToLauncher();
  };

  return (
    <div className="spectator-hud" aria-label="Spectator interface">
      <SpectatorTeamColumn title="Team A" tone="team-a" units={teamA} followActor={followActor} />
      <SpectatorTeamColumn title="Team B" tone="team-b" units={teamB} followActor={followActor} />
      <div className="spectator-bar" role="toolbar" aria-label="Spectator camera controls">
        <div className="spectator-bar-status">
          <span className="spectator-badge">Spectating</span>
          <strong className="spectator-view-label">
            {followedUnit ? `Following ${followedUnit.name}` : "Free camera"}
          </strong>
        </div>
        <div className="spectator-bar-actions">
          <Button
            size="1"
            variant={followActor === null ? "solid" : "soft"}
            color={followActor === null ? "indigo" : "gray"}
            aria-pressed={followActor === null}
            onClick={() => setSpectatorCamera("free")}
          >
            Free camera
          </Button>
          <Button
            size="1"
            variant="soft"
            color="gray"
            disabled={roster.length === 0}
            aria-label="Follow previous fighter"
            onClick={() => cycleSpectatorCamera(-1)}
          >
            Prev
          </Button>
          <Button
            size="1"
            variant="soft"
            color="gray"
            disabled={roster.length === 0}
            aria-label="Follow next fighter"
            onClick={() => cycleSpectatorCamera(1)}
          >
            Next
          </Button>
          <Button size="1" variant="soft" color="red" onClick={onLeave}>
            Leave
          </Button>
        </div>
        <span className="spectator-bar-hint">
          {followActor === null
            ? "WASD fly · Space rise · Drag look · Click any frame or nameplate to follow"
            : "Drag orbit · Wheel zoom · Click Free camera or another fighter"}
        </span>
      </div>
    </div>
  );
}
