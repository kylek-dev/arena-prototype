import { useRef } from "react";
import { visibleNameplates } from "../../domain/units/nameplates";
import { useConfigSelector } from "../../state/config";
import { draggingSpell, error, interaction, nameplateScene } from "../../state/session";
import { useStore } from "../../state/store";
import { interactUnit, unitRoster } from "../../runtime/commands";

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
  const selectedId = roster.target?.id;
  return (
    <div className="nameplates" aria-label="World health bars">
      {visibleNameplates(scene.nameplates ?? [], roster, preferences).map(
        ({ tag, unit, x, y, friendly }) => (
          <button
            key={tag}
            type="button"
            className={`nameplate ${friendly ? "friendly" : "enemy"}`}
            style={{ left: `${x * 100}%`, top: `${y * 100}%` }}
            aria-label={`${unit.name} health ${unit.health}/${unit.maxHealth}. Target; right-click to focus`}
            aria-pressed={unit.id === selectedId}
            onMouseDown={(e) => {
              press.current = { x: e.clientX, y: e.clientY };
            }}
            onClick={(e) => {
              if (
                !press.current ||
                Math.hypot(e.clientX - press.current.x, e.clientY - press.current.y) < 4
              )
                interactUnit(tag, "target");
              press.current = null;
            }}
            onContextMenu={(e) => {
              e.preventDefault();
              interactUnit(tag, "focus");
            }}
          >
            <span
              className="nameplate-fill"
              style={{ width: `${(unit.health / unit.maxHealth) * 100}%` }}
            />
            <span className="nameplate-name" aria-hidden="true">
              {unit.name}
            </span>
          </button>
        ),
      )}
    </div>
  );
}
