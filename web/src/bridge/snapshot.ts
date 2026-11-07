import type { Entity } from "../domain/units/selection";
export type Snapshot = {
  tick: number;
  frames: number;
  player: Entity & { x: number; z: number };
  target: Entity & { x: number; z: number };
  selected: boolean;
  selectedPlayer?: boolean;
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
    next.cooldown > previous.cooldown ||
    (next.cooldown === 0) !== (previous.cooldown === 0)
  );
}
// ABI v1 is the local two-entity practice adapter, not a multi-ability protocol.
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
    typeof r.won !== "boolean" ||
    typeof r.feedback !== "string" ||
    typeof r.attackStatus !== "string"
  )
    throw new Error("Invalid engine snapshot flags");
  const maxHealth = (raw as { maxHealth?: number }).maxHealth;
  if (!Number.isFinite(maxHealth) || !maxHealth || maxHealth < 1)
    throw new Error("Invalid maximum health");
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
    player: { ...r.player, maxHealth, id: "practice-player", name: "Player" },
    target: { ...r.target, maxHealth, id: "practice-opponent", name: "Enemy Bot" },
  };
}
declare global {
  interface Window {
    arenaShell: {
      hasCharacterModel?: boolean;
      readInput: () => number[];
      onState: (state: unknown) => void;
    };
    arenaDebug?: { readonly state: Snapshot | null; readonly metrics: unknown };
  }
}
