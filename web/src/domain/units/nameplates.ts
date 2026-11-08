import { resolveUnit, type UnitRoster } from "./tags";

export type NameplateProjection = {
  tag: `Opponent${1 | 2 | 3}` | `Team${1 | 2 | 3}`;
  x: number;
  y: number;
};

export function visibleNameplates(
  projections: readonly NameplateProjection[],
  roster: UnitRoster,
  preferences: { enemy: boolean; friendly: boolean },
) {
  return projections.flatMap((projection) => {
    const friendly = projection.tag.startsWith("Team");
    const unit = resolveUnit(projection.tag, roster);
    if (!preferences[friendly ? "friendly" : "enemy"] || !unit || unit.id === roster.player.id)
      return [];
    return [{ ...projection, unit, friendly }];
  });
}
