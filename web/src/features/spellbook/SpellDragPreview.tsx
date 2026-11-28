import { SPELLS } from "../../domain/abilities/catalog";
import { SpellIcon } from "../abilities/SpellIcon";
import { spellDragPreview } from "../../state/session";
import { useStore } from "../../state/store";

export function SpellDragPreview() {
  const preview = useStore(spellDragPreview, (state) => state);
  if (!preview) return null;
  const spell = SPELLS[preview.ability];
  return (
    <div
      className="action-slot spell-drag-preview"
      aria-hidden="true"
      // Centred on the pointer (the stylesheet shifts it by half its size).
      style={{ left: preview.x, top: preview.y }}
    >
      <SpellIcon spell={spell} />
      <small>{spell.name}</small>
    </div>
  );
}
