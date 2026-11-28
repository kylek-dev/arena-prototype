import type { Entity } from "./selection";

/** Units by role: in a match `Team1`–`Team2` and `Opponent1`–`Opponent3`;
 * in a hub world (ADR 0057) up to `Team39` (the other players) and the
 * training enemies as `Opponent1`… */
export type UnitTag = "Player" | "Target" | "Focus" | `Opponent${number}` | `Team${number}`;

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
  const index = Number(tag.replace(/^\D+/, "")) - 1;
  return (tag.startsWith("Opponent") ? roster.opponents : roster.team)[index] ?? null;
}
