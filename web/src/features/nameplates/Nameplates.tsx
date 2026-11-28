import { useRef, type RefObject } from "react";
import { spellLabel } from "../../domain/casting";
import { UnitCast } from "../cast-bar/UnitCast";
import { isLowHealth, Spark, useHealthChanges } from "../unit-frames/health-effects";
import { castEndings } from "../../runtime/cast-endings";
import { visibleNameplates } from "../../domain/units/nameplates";
import type { UnitTag } from "../../domain/units/tags";
import { shortName, type Entity } from "../../domain/units/selection";
import { useConfigSelector } from "../../state/config";
import { draggingSpell, error, interaction, nameplateScene } from "../../state/session";
import { useStore } from "../../state/store";
import { interactUnit, setSpectatorCamera, unitRoster } from "../../runtime/commands";
import { openUnitMenu } from "../../state/unit-menu";

export function Nameplates() {
  const scene = useStore(nameplateScene, (s) => s);
  const preferences = useConfigSelector((c) => c.nameplates);
  const mode = useStore(interaction, (s) => s);
  const dragging = useStore(draggingSpell, (s) => s);
  const failure = useStore(error, (s) => s);
  const press = useRef<{ x: number; y: number } | null>(null);
  if (
    !scene ||
    failure ||
    dragging ||
    mode.layout ||
    mode.quick ||
    mode.capture ||
    mode.panel === "menu" ||
    mode.panel === "settings"
  )
    return null;
  const roster = unitRoster(scene, null);
  const spectating = Boolean(scene.spectating);
  const selectedId = spectating ? null : roster.target?.id;
  return (
    <div className="nameplates" aria-label="World health bars">
      {visibleNameplates(scene.nameplates ?? [], roster, preferences, spectating).map(
        ({ tag, unit, x, y, friendly }) => {
          const actor = scene.roster?.find((entry) => entry.id === unit.id)?.actor ?? null;
          return (
            <Nameplate
              key={tag}
              tag={tag}
              unit={unit}
              x={x}
              y={y}
              friendly={friendly}
              spectating={spectating}
              pressed={
                spectating
                  ? actor !== null && scene.spectatorFollow === actor
                  : unit.id === selectedId
              }
              actor={actor}
              press={press}
            />
          );
        },
      )}
    </div>
  );
}

/** One unit's world health bar: health with its spark, heal wave and hit
 * trail, a low-health pulse, the selection's orbiting light, and the cast
 * bar with how the last cast ended. */
function Nameplate({
  tag,
  unit,
  x,
  y,
  friendly,
  spectating,
  pressed,
  actor,
  press,
}: {
  tag: UnitTag;
  unit: Entity;
  x: number;
  y: number;
  friendly: boolean;
  spectating: boolean;
  pressed: boolean;
  actor: number | null;
  press: RefObject<{ x: number; y: number } | null>;
}) {
  const activeCast = unit.cast ?? null;
  const ending = useStore(castEndings, (m) => m.has(unit.id));
  // An interrupt's ending plays first; the lockout label follows it.
  const lockedOut =
    !unit.disconnected && !activeCast && !ending && (unit.lockoutRemaining ?? 0) > 0;
  const health = Math.min(100, Math.max(0, (unit.health / unit.maxHealth) * 100));
  const { trail, heal, endTrail } = useHealthChanges(unit);
  return (
    <button
      type="button"
      className={`nameplate ${friendly ? "friendly" : "enemy"}${unit.disconnected ? " disconnected" : ""}${isLowHealth(unit) ? " low-health" : ""}${heal !== null ? " healing" : ""}`}
      style={{ left: `${x * 100}%`, top: `${y * 100}%` }}
      aria-label={`${unit.name}${unit.disconnected ? " (disconnected)" : ""} health ${unit.health}/${unit.maxHealth}${activeCast ? `, casting ${spellLabel(activeCast.spell)}` : ""}. ${spectating ? "Follow fighter" : friendly ? "Target; more options on right-click" : "Target; focus on right-click"}`}
      aria-pressed={pressed}
      onMouseDown={(e) => {
        press.current = { x: e.clientX, y: e.clientY };
      }}
      onClick={(e) => {
        if (
          !press.current ||
          Math.hypot(e.clientX - press.current.x, e.clientY - press.current.y) < 4
        ) {
          if (spectating && actor !== null) setSpectatorCamera(actor);
          else interactUnit(tag, "target");
        }
        press.current = null;
      }}
      onContextMenu={(e) => {
        e.preventDefault();
        if (spectating) return;
        // Enemies focus, as arena frames do; friends open the unit menu.
        if (friendly)
          openUnitMenu({ tag, unitId: unit.id, x: e.clientX, y: e.clientY, frame: null });
        else interactUnit(tag, "focus");
      }}
    >
      <span className="nameplate-fill" style={{ width: `${health}%` }}>
        {heal !== null && <span key={heal} className="health-heal-wave" />}
        <Spark percent={health} />
      </span>
      {trail && trail.from > trail.to && (
        <span
          key={trail.key}
          className="health-trail"
          style={{
            left: `${trail.to * 100}%`,
            width: `${(trail.from - trail.to) * 100}%`,
          }}
          onAnimationEnd={endTrail}
        />
      )}
      <span className="nameplate-name" aria-hidden="true">
        {unit.disconnected ? `${shortName(unit)} · Disconnected` : shortName(unit)}
      </span>
      <UnitCast unitId={unit.id} cast={activeCast} who={unit.name} variant="nameplate" />
      {lockedOut && (
        <span className="nameplate-lockout" aria-hidden="true">
          Locked
        </span>
      )}
    </button>
  );
}
