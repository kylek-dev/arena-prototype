import type { SpellMeta } from "../../domain/abilities/catalog";

/** The spell's painted icon, or its text symbol until it has one. Decorative: slots carry the name. */
export function SpellIcon({ spell }: { spell: SpellMeta }) {
  if (!spell.icon)
    return (
      <span className="spell-symbol" aria-hidden="true">
        {spell.symbol}
      </span>
    );
  return (
    <img
      className="spell-icon"
      src={spell.icon.x1}
      srcSet={`${spell.icon.x1} 1x, ${spell.icon.x2} 2x`}
      alt=""
      draggable={false}
    />
  );
}
