import { useConfigSelector, updateConfig } from "../../state/config";
import { engine } from "../../state/session";
import { useStore } from "../../state/store";
import { beginSpellPress } from "../../runtime/spell-drag";
import { ABILITY_IDS } from "../../domain/action-bars/model";
import { SPELLS } from "../../domain/abilities/catalog";
import { Hint } from "../../ui/Tooltip";
import { SpellDetails } from "../abilities/SpellDetails";
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
      <div className="spellbook-grid" role="group" aria-label="Known spells">
        {ABILITY_IDS.map((ability) => {
          const spell = SPELLS[ability];
          return (
            <Hint
              key={ability}
              text={
                <SpellDetails
                  ability={ability}
                  state={state}
                  instruction={
                    locked
                      ? "Assignments are locked. Unlock in Settings → Action bars."
                      : `Drag onto an action slot. This copies ${spell.name} and never casts it.`
                  }
                />
              }
            >
              <button
                className="action-slot spellbook-icon"
                aria-label={`${spell.name} spell`}
                data-spell-draggable={!locked}
                draggable={false}
                onPointerDown={(event) => beginSpellPress(event, { ability })}
                onDragStart={(event) => event.preventDefault()}
              >
                <span className="spell-symbol" aria-hidden="true">
                  {spell.symbol}
                </span>
              </button>
            </Hint>
          );
        })}
      </div>
    </div>
  );
}
