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
  /** A network match: the roster reports which players are attached. */
  networked?: boolean;
  nameplates?: NameplateProjection[];
  combatText?: CombatTextProjection[];
  distance: number;
  range: number;
  cooldown: number;
  attackStatus: string;
  feedback: string;
  won: boolean;
  character: { status: string };
  /** Render warmup behind the loading screen; absent from older clients. */
  startup?: StartupState;
  camera: { yaw: number; pitch: number; distance: number };
  pulse: { damage: number; cooldown: number; range: number };
};

export type StartupState = {
  phase: "assets" | "render" | "warming" | "ready";
  ready: boolean;
  pending: number;
  timedOut: boolean;
};

const STARTUP_PHASES = new Set(["assets", "render", "warming", "ready"]);
const CHARACTER_STATUSES = new Set(["ready", "failed", "placeholder", "loading"]);
const HOVERED_UNIT_RE = /^(Player|Opponent[1-3]|Team[1-2])$/;
const NAMEPLATE_TAG_RE = /^(Opponent|Team)[1-3]$/;

function isNonNegativeFinite(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value) && value >= 0;
}

function validStartup(value: unknown): value is StartupState {
  const s = value as StartupState;
  return (
    !!s &&
    STARTUP_PHASES.has(s.phase) &&
    typeof s.ready === "boolean" &&
    s.ready === (s.phase === "ready") &&
    Number.isInteger(s.pending) &&
    s.pending >= 0 &&
    typeof s.timedOut === "boolean"
  );
}

export function needsImmediateDelivery(previous: Snapshot | null, next: Snapshot) {
  return (
    !previous ||
    next.tick < previous.tick ||
    next.teamSize !== previous.teamSize ||
    next.selectedActor !== previous.selectedActor ||
    next.won !== previous.won ||
    next.startup?.ready !== previous.startup?.ready ||
    next.hoveredUnit !== previous.hoveredUnit ||
    next.cooldown > previous.cooldown ||
    (next.cooldown === 0) !== (previous.cooldown === 0)
  );
}
// Local practice snapshots optionally append a team roster; not a network protocol.
export function adaptSnapshot(raw: unknown): Snapshot {
  if (!raw || typeof raw !== "object") throw new Error("Invalid engine snapshot");
  const r = raw as Snapshot;
  if (
    !isNonNegativeFinite(r.tick) ||
    !isNonNegativeFinite(r.frames) ||
    !isNonNegativeFinite(r.player?.health) ||
    !isNonNegativeFinite(r.target?.health) ||
    !isNonNegativeFinite(r.cooldown) ||
    !isNonNegativeFinite(r.distance) ||
    !isNonNegativeFinite(r.range) ||
    !isNonNegativeFinite(r.pulse?.damage) ||
    !isNonNegativeFinite(r.pulse?.cooldown) ||
    !isNonNegativeFinite(r.pulse?.range)
  ) {
    throw new Error("Invalid engine snapshot values");
  }
  if (
    !Number.isFinite(r.player?.x) ||
    !Number.isFinite(r.player?.z) ||
    !Number.isFinite(r.target?.x) ||
    !Number.isFinite(r.target?.z) ||
    !Number.isFinite(r.camera?.yaw) ||
    !Number.isFinite(r.camera?.pitch) ||
    !Number.isFinite(r.camera?.distance)
  ) {
    throw new Error("Invalid engine coordinates");
  }
  if (!CHARACTER_STATUSES.has(r.character?.status)) {
    throw new Error("Invalid character loading status");
  }
  if (r.startup !== undefined && !validStartup(r.startup)) throw new Error("Invalid startup state");
  if (
    typeof r.selected !== "boolean" ||
    (r.selectedPlayer !== undefined && typeof r.selectedPlayer !== "boolean") ||
    (r.hoveredUnit !== undefined &&
      r.hoveredUnit !== null &&
      !HOVERED_UNIT_RE.test(r.hoveredUnit)) ||
    typeof r.won !== "boolean" ||
    typeof r.feedback !== "string" ||
    typeof r.attackStatus !== "string"
  )
    throw new Error("Invalid engine snapshot flags");
  const maxHealth = (raw as { maxHealth?: number }).maxHealth;
  if (r.combatText !== undefined) {
    if (!Array.isArray(r.combatText) || r.combatText.length > 32)
      throw new Error("Invalid combat text projections");
    const len = r.combatText.length;
    if (len > 0) {
      for (let i = 0; i < len; i += 1) {
        const entry = r.combatText[i];
        if (!validCombatText(entry)) throw new Error("Invalid combat text projections");
        for (let j = 0; j < i; j += 1) {
          if (r.combatText[j].id === entry.id) throw new Error("Invalid combat text projections");
        }
      }
    }
  }
  if (r.nameplates !== undefined) {
    if (!Array.isArray(r.nameplates) || r.nameplates.length > 6)
      throw new Error("Invalid nameplate projections");
    const len = r.nameplates.length;
    for (let i = 0; i < len; i += 1) {
      const plate = r.nameplates[i];
      if (
        !plate ||
        !NAMEPLATE_TAG_RE.test(plate.tag) ||
        !Number.isFinite(plate.x) ||
        !Number.isFinite(plate.y) ||
        plate.x < 0 ||
        plate.x > 1 ||
        plate.y < 0 ||
        plate.y > 1
      )
        throw new Error("Invalid nameplate projection");
      for (let j = 0; j < i; j += 1) {
        if (r.nameplates[j].tag === plate.tag) throw new Error("Invalid nameplate projection");
      }
    }
  }
  if (!Number.isFinite(maxHealth) || !maxHealth || maxHealth < 1)
    throw new Error("Invalid maximum health");
  let roster: RosterEntry[] | undefined;
  let networked = false;
  if (r.roster !== undefined) {
    const teamSize = r.teamSize;
    if (
      (teamSize !== 1 && teamSize !== 2 && teamSize !== 3) ||
      !Array.isArray(r.roster) ||
      r.roster.length !== teamSize * 2
    )
      throw new Error("Invalid practice roster");
    let anyConnected = false;
    let allBoolean = true;
    for (let i = 0; i < r.roster.length; i += 1) {
      const value = (r.roster[i] as { connected?: unknown }).connected;
      if (value !== undefined) anyConnected = true;
      if (typeof value !== "boolean") allBoolean = false;
    }
    networked = anyConnected;
    if (networked && !allBoolean) throw new Error("Invalid connection flags");
    roster = new Array<RosterEntry>(r.roster.length);
    for (let actor = 0; actor < r.roster.length; actor += 1) {
      const unit = r.roster[actor];
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
      const conn = (unit as { connected?: boolean }).connected;
      roster[actor] = {
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
              : networked
                ? `Opponent ${Math.floor(actor / 2) + 1}`
                : `Enemy Bot${actor === 1 ? "" : ` ${Math.floor(actor / 2) + 1}`}`,
        ...(conn === false ? { disconnected: true } : {}),
      };
    }
    if (
      r.selectedActor !== null &&
      (!Number.isInteger(r.selectedActor) ||
        (r.selectedActor as number) < 0 ||
        (r.selectedActor as number) >= roster.length)
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
  const opponent = roster?.[1];
  return {
    ...r,
    roster,
    networked,
    player: { ...r.player, maxHealth, id: "practice-player", name: "Player" },
    target: {
      ...r.target,
      maxHealth,
      id: "practice-opponent",
      name: opponent?.name ?? "Enemy Bot",
      ...(opponent?.disconnected ? { disconnected: true } : {}),
    },
  };
}
declare global {
  interface Window {
    arenaShell: {
      characterModel?: string;
      readInput: () => number[];
      readHover?: () => number[];
      readGraphics?: () => number[];
      readWeapons?: () => number[];
      readLighting?: () => number[];
      readMap?: () => unknown[];
      readAppearance?: () => unknown[];
      readPreview?: () => number[];
      onMapStatus?: (status: unknown) => void;
      onFocusUnit?: (tag: UnitTag) => void;
      /** Network match feed: `[generation, ...frames]`, or empty without a match. */
      readNetwork?: () => unknown[];
      sendNetwork?: (frame: Uint8Array) => void;
      onNetwork?: (status: unknown) => void;
      onState: (state: unknown) => void;
    };
    arenaDebug?: { readonly state: Snapshot | null; readonly metrics: unknown };
  }
}
