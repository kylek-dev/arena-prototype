import { PULSE, availability } from "../../domain/abilities/catalog";
import { useConfigSelector, updateConfig } from "../../state/config";
import { engine } from "../../state/session";
import { useStore } from "../../state/store";
import { beginSpellPress } from "../../runtime/spell-drag";
import { Hint } from "../../ui/Tooltip";
import { Button } from "@radix-ui/themes";

export function Spellbook() {
  const state = useStore(engine, (s) => s);
  const locked = useConfigSelector((c) => c.assignmentsLocked);

  return (
    <div className="stack">
      {locked && (
        <Button
          variant="soft"
          onClick={() => updateConfig((c) => ({ ...c, assignmentsLocked: false }))}
        >
          Unlock spell dragging
        </Button>
      )}
      <div className="spellbook-grid" aria-label="Known spells">
        <Hint
          text={
            <>
              <strong>{PULSE.name}</strong>
              <p>{PULSE.description}</p>
              {state && (
                <p>
                  {state.pulse.damage} damage · {state.pulse.range} m · {state.pulse.cooldown} s
                  recharge
                </p>
              )}
              <p>{availability(state)}</p>
              <p>
                {locked
                  ? "Assignments are locked. Unlock in Settings → Action bars."
                  : "Drag onto an action slot. This copies Pulse and never casts it."}
              </p>
            </>
          }
        >
          <button
            className="action-slot spellbook-icon"
            aria-label="Pulse spell"
            data-spell-draggable={!locked}
            draggable={false}
            onPointerDown={(event) => beginSpellPress(event, { ability: "pulse" })}
            onDragStart={(event) => event.preventDefault()}
          >
            <span className="spell-symbol" aria-hidden="true">
              ϟ
            </span>
            <small>Pulse</small>
          </button>
        </Hint>
      </div>
    </div>
  );
}
