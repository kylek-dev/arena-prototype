import type { Slot } from "../../domain/action-bars/model";
import { SPELLS, actionTint, availability, spellCooldown } from "../../domain/abilities/catalog";
import { label } from "../../domain/bindings/model";
import { useConfigSelector } from "../../state/config";
import { engine, heldSlots, interaction, spellDragPreview } from "../../state/session";
import { useStore } from "../../state/store";
import { activateSlot } from "../../runtime/commands";
import { Hint, TooltipCard } from "../../ui/Tooltip";
import { SpellDetails } from "../abilities/SpellDetails";
import { beginSpellPress } from "../../runtime/spell-drag";
import { useRef } from "react";
import { Cooldown } from "./Cooldown";
export function ActionSlot({ slot, number }: { slot: Slot; number: number }) {
  const state = useStore(engine, (s) => s),
    mode = useStore(interaction, (s) => s);
  const locked = useConfigSelector((c) => c.assignmentsLocked),
    pair = useConfigSelector((c) => c.bindings[`slot:${slot.id}`]);
  const ability = slot.ability;
  const meta = ability ? SPELLS[ability] : null;
  const status = ability ? availability(state, ability) : "";
  const cooling = !!ability && spellCooldown(state, ability) > 0;
  const eligibility = useRef({ pointer: false, focus: false });
  const dropTarget = useStore(spellDragPreview, (preview) => preview?.target === slot.id);
  const keyHeld = useStore(heldSlots, (slots) => slots.has(slot.id));
  function hover(kind: "pointer" | "focus", active: boolean) {
    eligibility.current[kind] = active;
    const eligible = eligibility.current.pointer || eligibility.current.focus;
    const m = interaction.get();
    if (m.quick)
      interaction.set({
        ...m,
        hovered: eligible ? slot.id : m.hovered === slot.id ? null : m.hovered,
      });
  }
  return (
    <Hint
      text={
        ability ? (
          <SpellDetails
            ability={ability}
            state={state}
            shortcut={pair?.filter(Boolean).map(label).join(" / ") || "Unbound"}
            instruction={
              locked
                ? "Assignments locked. Unlock in Settings → Action bars."
                : "Drag to move or swap · Shift-drag to copy. Drop on the arena to remove."
            }
          />
        ) : (
          <TooltipCard
            title="Empty slot"
            shortcut={pair?.filter(Boolean).map(label).join(" / ") || "Unbound"}
            description="Place a spell from Spells."
            footer={locked ? "Assignments locked. Unlock in Settings → Action bars." : undefined}
          />
        )
      }
    >
      <button
        className={`action-slot ${mode.quick && mode.hovered === slot.id ? "binding-hover" : ""} ${dropTarget ? "drop-target" : ""} ${ability ? "" : "empty"} ${keyHeld ? "key-held" : ""}`}
        data-spell-drop-slot={slot.id}
        data-unusable={ability ? actionTint(state, ability) : undefined}
        data-spell-draggable={!locked && !!ability && !mode.quick && !mode.layout}
        aria-label={`Slot ${number} ${meta ? meta.name : "empty"}`}
        aria-disabled={!ability || status !== "Ready"}
        draggable={false}
        onMouseEnter={() => hover("pointer", true)}
        onMouseLeave={() => hover("pointer", false)}
        onFocus={() => hover("focus", true)}
        onBlur={() => hover("focus", false)}
        onPointerDown={(event) => {
          if (!locked && ability) beginSpellPress(event, { ability, source: slot.id });
        }}
        onClick={() => {
          if (!mode.quick && !mode.layout && !mode.panel) activateSlot(slot.id);
        }}
        onDragStart={(event) => event.preventDefault()}
      >
        <kbd>{pair?.filter(Boolean).map(label).join(" / ")}</kbd>
        <span className="spell-symbol" aria-hidden="true">
          {meta ? meta.symbol : "·"}
        </span>
        {ability && cooling && <Cooldown ability={ability} />}
      </button>
    </Hint>
  );
}
