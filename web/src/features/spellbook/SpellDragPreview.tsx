import { spellDragPreview } from "../../state/session";
import { useStore } from "../../state/store";

export function SpellDragPreview() {
  const preview = useStore(spellDragPreview, (state) => state);
  if (!preview) return null;
  return (
    <div
      className="action-slot spell-drag-preview"
      aria-hidden="true"
      style={{ left: preview.x + 12, top: preview.y + 12 }}
    >
      <span className="spell-symbol">ϟ</span>
      <small>Pulse</small>
    </div>
  );
}
