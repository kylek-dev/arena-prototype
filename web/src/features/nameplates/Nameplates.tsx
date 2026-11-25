import { useRef } from "react";
import { spellLabel } from "../../domain/casting";
import { CastBar } from "../cast-bar/CastBar";
import { visibleNameplates } from "../../domain/units/nameplates";
import { shortName } from "../../domain/units/selection";
import { useConfigSelector } from "../../state/config";
import { draggingSpell, error, interaction, nameplateScene } from "../../state/session";
import { useStore } from "../../state/store";
import { interactUnit, setSpectatorCamera, unitRoster } from "../../runtime/commands";

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
          const activeCast = unit.cast ?? null;
          const lockedOut = !unit.disconnected && !activeCast && (unit.lockoutRemaining ?? 0) > 0;
          const actor = scene.roster?.find((entry) => entry.id === unit.id)?.actor ?? null;
          const pressed = spectating
            ? actor !== null && scene.spectatorFollow === actor
            : unit.id === selectedId;
          return (
            <button
              key={tag}
              type="button"
              className={`nameplate ${friendly ? "friendly" : "enemy"}${unit.disconnected ? " disconnected" : ""}`}
              style={{ left: `${x * 100}%`, top: `${y * 100}%` }}
              aria-label={`${unit.name}${unit.disconnected ? " (disconnected)" : ""} health ${unit.health}/${unit.maxHealth}${activeCast ? `, casting ${spellLabel(activeCast.spell)}` : ""}. ${spectating ? "Follow fighter" : "Target; right-click to focus"}`}
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
                if (!spectating) interactUnit(tag, "focus");
              }}
            >
              <span
                className="nameplate-fill"
                style={{ width: `${(unit.health / unit.maxHealth) * 100}%` }}
              />
              <span className="nameplate-name" aria-hidden="true">
                {unit.disconnected ? `${shortName(unit)} · Disconnected` : shortName(unit)}
              </span>
              {activeCast && <CastBar variant="nameplate" cast={activeCast} />}
              {lockedOut && (
                <span className="nameplate-lockout" aria-hidden="true">
                  Locked
                </span>
              )}
            </button>
          );
        },
      )}
    </div>
  );
}
