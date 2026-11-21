import type { Snapshot } from "../../bridge/snapshot";
import type { AbilityId } from "../../domain/action-bars/model";
import { SPELLS, availability } from "../../domain/abilities/catalog";
import { TooltipCard } from "../../ui/Tooltip";

// Shared by spellbook and slotted spells. Combat values only come from the engine.
export function SpellDetails({
  ability = "pulse",
  state,
  shortcut,
  instruction,
}: {
  ability?: AbilityId;
  state: Snapshot | null;
  shortcut?: string;
  instruction: string;
}) {
  const meta = SPELLS[ability];
  const spec = state?.spells?.[ability];
  const facts = state
    ? spec
      ? [
          ...(ability === "mend"
            ? [{ label: "Heal", value: spec.power }]
            : ability === "quell"
              ? [{ label: "Lockout", value: "2 s" }]
              : [{ label: "Damage", value: spec.power }]),
          { label: "Cost", value: `${spec.cost} mana` },
          { label: "Range", value: `${spec.range} m` },
          { label: "Cast", value: spec.castTime > 0 ? `${spec.castTime} s` : "Instant" },
          { label: "Recharge", value: `${spec.maxCooldown} s` },
        ]
      : ability === "pulse"
        ? [
            { label: "Damage", value: state.pulse.damage },
            { label: "Range", value: `${state.pulse.range} m` },
            { label: "Recharge", value: `${state.pulse.cooldown} s` },
          ]
        : undefined
    : undefined;
  return (
    <TooltipCard
      title={meta.name}
      shortcut={shortcut}
      description={meta.description}
      facts={facts}
      status={availability(state, ability)}
      footer={instruction}
    />
  );
}
