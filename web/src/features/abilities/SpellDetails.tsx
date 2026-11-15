import type { Snapshot } from "../../bridge/snapshot";
import { PULSE, availability } from "../../domain/abilities/catalog";
import { TooltipCard } from "../../ui/Tooltip";

// Shared by spellbook and slotted spells. Combat values only come from the engine.
export function SpellDetails({
  state,
  shortcut,
  instruction,
}: {
  state: Snapshot | null;
  shortcut?: string;
  instruction: string;
}) {
  return (
    <TooltipCard
      title={PULSE.name}
      shortcut={shortcut}
      description={PULSE.description}
      facts={
        state
          ? [
              { label: "Damage", value: state.pulse.damage },
              { label: "Range", value: `${state.pulse.range} m` },
              { label: "Recharge", value: `${state.pulse.cooldown} s` },
            ]
          : undefined
      }
      status={availability(state)}
      footer={instruction}
    />
  );
}
