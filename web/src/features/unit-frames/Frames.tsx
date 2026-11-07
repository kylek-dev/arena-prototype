import { useConfigSelector, updateConfig } from "../../state/config";
import { engine, focus, interaction } from "../../state/session";
import { useStore } from "../../state/store";
import { dispatch } from "../../runtime/commands";
import { Placed } from "../../ui/Placed";
import { UnitFrame } from "./UnitFrame";
import { Button } from "@radix-ui/themes";
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
              name: role === "player" ? "Contender" : "Sentinel",
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
            <UnitFrame
              role={role}
              unit={unit ?? null}
              selected={unit?.id === state?.target.id && state?.selected}
              focused={unit?.id === focused}
              preview={editing}
            />
            {!editing && role === "target" && (
              <Button size="1" variant="ghost" color="gray" onClick={() => dispatch("setFocus")}>
                Set focus
              </Button>
            )}
            {!editing && role === "focus" && (
              <div className="row">
                <Button
                  size="1"
                  variant="ghost"
                  color="gray"
                  onClick={() => dispatch("targetFocus")}
                >
                  Target focus
                </Button>
                <Button
                  size="1"
                  variant="ghost"
                  color="gray"
                  onClick={() => dispatch("clearFocus")}
                >
                  Clear focus
                </Button>
              </div>
            )}
          </Placed>
        );
      })}
    </>
  );
}
