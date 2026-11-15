import { resolveUnit } from "../../domain/units/tags";
import { unitRoster } from "../../runtime/commands";
import { hoveredFrame } from "../../runtime/unit-hover";
import { updateConfig, useConfigSelector } from "../../state/config";
import { draggingSpell, engine, error, focus, interaction } from "../../state/session";
import { useStore } from "../../state/store";
import { Placed } from "../../ui/Placed";
import { TooltipCard } from "../../ui/Tooltip";

export function UnitTooltip() {
  const state = useStore(engine, (s) => s);
  const focused = useStore(focus, (s) => s);
  const tag = useStore(hoveredFrame, (s) => s);
  const mode = useStore(interaction, (s) => s);
  const dragging = useStore(draggingSpell, (s) => s);
  const failed = useStore(error, (s) => s);
  const value = useConfigSelector((c) => c.frames.unitTooltip);
  const activeTag = tag ?? state?.hoveredUnit;
  const unit = mode.layout
    ? { id: "sample-tooltip", name: "Enemy Bot", health: 75, maxHealth: 100 }
    : state && activeTag
      ? resolveUnit(activeTag, unitRoster(state, focused))
      : null;
  const suppressed =
    dragging ||
    mode.quick ||
    !!mode.capture ||
    mode.panel === "menu" ||
    mode.panel === "settings" ||
    !!failed;
  if (!mode.layout && (!unit || suppressed)) return null;
  if (!unit) return null;
  return (
    <Placed
      id="unitTooltip"
      value={value}
      onChange={(next) =>
        updateConfig((c) => ({ ...c, frames: { ...c.frames, unitTooltip: next } }))
      }
    >
      <div
        className={`tooltip unit-tooltip ${mode.layout ? "" : "unit-tooltip-live"}`}
        role={mode.layout ? undefined : "tooltip"}
        id="unit-tooltip"
      >
        <TooltipCard
          title={unit.name}
          facts={[
            { label: "Health", value: `${unit.health}/${unit.maxHealth}` },
            { label: "Resource", value: "100/100" },
          ]}
          status={unit.health === 0 ? "Defeated" : undefined}
          footer={
            mode.layout
              ? "Unit tooltip · layout preview"
              : "Resource placeholder · no resource costs or regeneration yet"
          }
        />
      </div>
    </Placed>
  );
}
