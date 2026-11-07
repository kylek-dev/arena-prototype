import type { Entity } from "./selection";

export type UnitTag = "Player" | "Target" | "Focus" | `Opponent${1 | 2 | 3}` | `Team${1 | 2 | 3}`;

export type UnitRoster = {
  player: Entity;
  target: Entity | null;
  focus: string | null;
  opponents: readonly Entity[];
  team: readonly Entity[];
};

export function resolveUnit(tag: UnitTag, roster: UnitRoster): Entity | null {
  if (tag === "Player") return roster.player;
  if (tag === "Target") return roster.target;
  if (tag === "Focus")
    return (
      [roster.player, ...roster.opponents, ...roster.team].find(
        (unit) => unit.id === roster.focus,
      ) ?? null
    );
  const index = Number(tag.at(-1)) - 1;
  return (tag.startsWith("Opponent") ? roster.opponents : roster.team)[index] ?? null;
}
