import { SPELLS } from "../../domain/abilities/catalog";
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
      style={{ left: preview.x + 12, top: preview.y + 12 }}
    >
      <span className="spell-symbol">{spell.symbol}</span>
      <small>{spell.name}</small>
    </div>
  );
}
