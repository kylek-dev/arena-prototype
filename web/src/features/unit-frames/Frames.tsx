import { useConfigSelector, updateConfig } from "../../state/config";
import { engine, focus, interaction } from "../../state/session";
import { useStore } from "../../state/store";
import { interactUnit, unitRoster } from "../../runtime/commands";
import { resolveUnit, type UnitTag } from "../../domain/units/tags";
import type { Entity } from "../../domain/units/selection";
import { useEffect, type ComponentProps } from "react";
import { Placed } from "../../ui/Placed";
import { UnitFrame } from "./UnitFrame";
import { hoveredFrame } from "../../runtime/unit-hover";

function TaggedFrame({
  tag,
  role,
  unit,
  editing,
  selected,
}: {
  tag: UnitTag;
  role: ComponentProps<typeof UnitFrame>["role"];
  unit: Entity | null;
  editing: boolean;
  selected: boolean;
}) {
  const hovered = useStore(hoveredFrame, (s) => s === tag);
  useEffect(
    () => () => {
      if (hoveredFrame.get() === tag) hoveredFrame.set(null);
    },
    [tag, unit?.id],
  );
  const frame = <UnitFrame role={role} unit={unit} />;
  if (editing) return frame;
  return (
    <button
      type="button"
      className="unit-entry"
      data-unit-tag={tag}
      aria-describedby={hovered ? "unit-tooltip" : undefined}
      aria-pressed={selected}
      aria-label={
        tag.startsWith("Opponent")
          ? `Target ${unit?.name ?? tag}`
          : `Target ${tag}: ${unit?.name ?? "—"}`
      }
      onMouseEnter={() => hoveredFrame.set(unit ? tag : null)}
      onMouseLeave={() => {
        if (hoveredFrame.get() === tag) hoveredFrame.set(null);
      }}
      onFocus={() => hoveredFrame.set(unit ? tag : null)}
      onBlur={() => {
        if (hoveredFrame.get() === tag) hoveredFrame.set(null);
      }}
      disabled={!unit}
      onClick={() => interactUnit(tag, "target")}
      onContextMenu={(e) => {
        e.preventDefault();
        interactUnit(tag, "focus");
      }}
    >
      {frame}
    </button>
  );
}

export function Frames() {
  const state = useStore(engine, (s) => s),
    focused = useStore(focus, (s) => s),
    editing = useStore(interaction, (s) => s.layout);
  const frames = useConfigSelector((c) => c.frames);
  const selectedId = state ? unitRoster(state, focused).target?.id : null;
  return (
    <>
      {(["player", "target", "focus"] as const).map((role) => {
        const tag = { player: "Player", target: "Target", focus: "Focus" }[role] as UnitTag;
        const liveUnit = state ? resolveUnit(tag, unitRoster(state, focused)) : null;
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
            <TaggedFrame
              tag={tag}
              role={role}
              unit={unit ?? null}
              editing={editing}
              selected={!!unit && unit.id === selectedId}
            />
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
            <TaggedFrame
              key={unit.id}
              tag="Opponent1"
              role="opponent"
              unit={unit}
              editing={editing}
              selected={unit.id === selectedId}
            />
          ))}
        </div>
      </Placed>
    </>
  );
}
