import type { Entity } from "../domain/units/selection";
import type { UnitTag } from "../domain/units/tags";
export type RosterEntry = Entity & { actor: number; tag: UnitTag };
import type { NameplateProjection } from "../domain/units/nameplates";
import { validCombatText, type CombatTextProjection } from "../domain/units/combat-text";
export type Snapshot = {
  tick: number;
  frames: number;
  player: Entity & { x: number; z: number };
  target: Entity & { x: number; z: number };
  selected: boolean;
  selectedPlayer?: boolean;
  hoveredUnit?: UnitTag | null;
  teamSize?: 1 | 2 | 3;
  selectedActor?: number | null;
  roster?: RosterEntry[];
  nameplates?: NameplateProjection[];
  combatText?: CombatTextProjection[];
  distance: number;
  range: number;
  cooldown: number;
  attackStatus: string;
  feedback: string;
  won: boolean;
  character: { status: string };
  camera: { yaw: number; pitch: number; distance: number };
  pulse: { damage: number; cooldown: number; range: number };
};

export function needsImmediateDelivery(previous: Snapshot | null, next: Snapshot) {
  return (
    !previous ||
    next.tick < previous.tick ||
    next.teamSize !== previous.teamSize ||
    next.selectedActor !== previous.selectedActor ||
    next.won !== previous.won ||
    next.hoveredUnit !== previous.hoveredUnit ||
    next.cooldown > previous.cooldown ||
    (next.cooldown === 0) !== (previous.cooldown === 0)
  );
}
// Local practice snapshots optionally append a team roster; not a network protocol.
export function adaptSnapshot(raw: unknown): Snapshot {
  if (!raw || typeof raw !== "object") throw new Error("Invalid engine snapshot");
  const r = raw as Snapshot;
  for (const n of [
    r.tick,
    r.frames,
    r.player?.health,
    r.target?.health,
    r.cooldown,
    r.distance,
    r.range,
    r.pulse?.damage,
    r.pulse?.cooldown,
    r.pulse?.range,
  ])
    if (!Number.isFinite(n) || n < 0) throw new Error("Invalid engine snapshot values");
  for (const n of [
    r.player?.x,
    r.player?.z,
    r.target?.x,
    r.target?.z,
    r.camera?.yaw,
    r.camera?.pitch,
    r.camera?.distance,
  ]) {
    if (!Number.isFinite(n)) throw new Error("Invalid engine coordinates");
  }
  if (!["ready", "failed", "placeholder", "loading"].includes(r.character?.status)) {
    throw new Error("Invalid character loading status");
  }
  if (
    typeof r.selected !== "boolean" ||
    (r.selectedPlayer !== undefined && typeof r.selectedPlayer !== "boolean") ||
    (r.hoveredUnit !== undefined &&
      r.hoveredUnit !== null &&
      !/^(Player|Opponent[1-3]|Team[1-2])$/.test(r.hoveredUnit)) ||
    typeof r.won !== "boolean" ||
    typeof r.feedback !== "string" ||
    typeof r.attackStatus !== "string"
  )
    throw new Error("Invalid engine snapshot flags");
  const maxHealth = (raw as { maxHealth?: number }).maxHealth;
  if (r.combatText !== undefined) {
    if (
      !Array.isArray(r.combatText) ||
      r.combatText.length > 32 ||
      !r.combatText.every(validCombatText) ||
      new Set(r.combatText.map((entry) => entry.id)).size !== r.combatText.length
    )
      throw new Error("Invalid combat text projections");
  }
  if (r.nameplates !== undefined) {
    if (!Array.isArray(r.nameplates) || r.nameplates.length > 6)
      throw new Error("Invalid nameplate projections");
    const tags = new Set<string>();
    for (const plate of r.nameplates) {
      if (
        !plate ||
        !/^(Opponent|Team)[1-3]$/.test(plate.tag) ||
        tags.has(plate.tag) ||
        !Number.isFinite(plate.x) ||
        !Number.isFinite(plate.y) ||
        plate.x < 0 ||
        plate.x > 1 ||
        plate.y < 0 ||
        plate.y > 1
      )
        throw new Error("Invalid nameplate projection");
      tags.add(plate.tag);
    }
  }
  if (!Number.isFinite(maxHealth) || !maxHealth || maxHealth < 1)
    throw new Error("Invalid maximum health");
  let roster: RosterEntry[] | undefined;
  if (r.roster !== undefined) {
    if (
      ![1, 2, 3].includes(r.teamSize ?? 0) ||
      !Array.isArray(r.roster) ||
      r.roster.length !== (r.teamSize ?? 0) * 2
    )
      throw new Error("Invalid practice roster");
    roster = r.roster.map((unit, actor) => {
      const tag =
        actor === 0
          ? "Player"
          : actor % 2 === 0
            ? `Team${actor / 2}`
            : `Opponent${Math.floor(actor / 2) + 1}`;
      if (
        unit.actor !== actor ||
        unit.tag !== tag ||
        !Number.isFinite(unit.health) ||
        unit.health < 0 ||
        unit.health > maxHealth
      )
        throw new Error("Invalid practice unit");
      return {
        actor,
        tag: tag as UnitTag,
        health: unit.health,
        maxHealth,
        id:
          actor === 0
            ? "practice-player"
            : actor === 1
              ? "practice-opponent"
              : `practice-actor-${actor}`,
        name:
          actor === 0
            ? "Player"
            : actor % 2 === 0
              ? `Teammate ${actor / 2}`
              : `Enemy Bot${actor === 1 ? "" : ` ${Math.floor(actor / 2) + 1}`}`,
      };
    });
    if (
      r.selectedActor !== null &&
      (!Number.isInteger(r.selectedActor) || !roster.some((unit) => unit.actor === r.selectedActor))
    )
      throw new Error("Invalid selected actor");
    if (r.selected !== (r.selectedActor !== null) || r.selectedPlayer !== (r.selectedActor === 0))
      throw new Error("Inconsistent selection");
  }
  if (
    r.player.health > maxHealth ||
    r.target.health > maxHealth ||
    r.pulse.damage <= 0 ||
    r.pulse.cooldown <= 0
  ) {
    throw new Error("Invalid engine rule metadata");
  }
  return {
    ...r,
    roster,
    player: { ...r.player, maxHealth, id: "practice-player", name: "Player" },
    target: { ...r.target, maxHealth, id: "practice-opponent", name: "Enemy Bot" },
  };
}
declare global {
  interface Window {
    arenaShell: {
      hasCharacterModel?: boolean;
      readInput: () => number[];
      readHover?: () => number[];
      readGraphics?: () => number[];
      readWeapons?: () => number[];
      readLighting?: () => number[];
      onFocusUnit?: (tag: UnitTag) => void;
      onState: (state: unknown) => void;
    };
    arenaDebug?: { readonly state: Snapshot | null; readonly metrics: unknown };
  }
}
