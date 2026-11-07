import { useConfigSelector, updateConfig } from "../../state/config";
import { engine, focus, interaction } from "../../state/session";
import { useStore } from "../../state/store";
import { interactOpponent } from "../../runtime/commands";
import { Placed } from "../../ui/Placed";
import { UnitFrame } from "./UnitFrame";
export function Frames() {
  const state = useStore(engine, (s) => s),
    focused = useStore(focus, (s) => s),
    editing = useStore(interaction, (s) => s.layout);
  const frames = useConfigSelector((c) => c.frames);
  return (
    <>
      {(["player", "target", "focus"] as const).map((role) => {
        const liveUnit =
          role === "player"
            ? state?.player
            : role === "target"
              ? state?.selected
                ? state.target
                : null
              : state?.target.id === focused
                ? state.target
                : null;
        if (!editing && role !== "player" && !liveUnit) return null;
        const maximum = state?.player.maxHealth ?? 100;
        const unit = editing
          ? {
              id: `sample-${role}`,
              name: role === "player" ? "Player" : "Enemy Bot",
              health: Math.round(maximum * 0.75),
              maxHealth: maximum,
            }
          : liveUnit;
        return (
          <Placed
            key={role}
            id={role}
            value={frames[role]}
            onChange={(l) => updateConfig((c) => ({ ...c, frames: { ...c.frames, [role]: l } }))}
          >
            <UnitFrame role={role} unit={unit ?? null} />
          </Placed>
        );
      })}
      <Placed
        id="opponents"
        value={frames.opponents}
        onChange={(l) => updateConfig((c) => ({ ...c, frames: { ...c.frames, opponents: l } }))}
      >
        <div className="opponent-list" role="group" aria-label="Opponents">
          {(editing
            ? [{ id: "sample-opponent", name: "Enemy Bot", health: 75, maxHealth: 100 }]
            : state
              ? [state.target]
              : []
          ).map((unit) => (
            <button
              key={unit.id}
              type="button"
              className="opponent-entry"
              aria-label={`Target ${unit.name}`}
              aria-pressed={state?.selected && state.target.id === unit.id}
              title="Left-click to target; right-click to focus"
              onClick={() => interactOpponent(unit.id, "target")}
              onContextMenu={(e) => {
                e.preventDefault();
                interactOpponent(unit.id, "focus");
              }}
            >
              <UnitFrame role="opponent" unit={unit} />
            </button>
          ))}
        </div>
      </Placed>
    </>
  );
}
