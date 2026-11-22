import { isValidActiveCast, type ActiveCast, type SpellKey } from "../domain/casting";
import { snapshotRemainingSeconds } from "../domain/round-clock";
import {
  expectedActorTag,
  isNonNegativeSafeInt,
  isValidScoreboardEntry,
  type ScoreboardEntry,
} from "../domain/scoreboard";
import type { Entity } from "../domain/units/selection";
import type { UnitTag } from "../domain/units/tags";
export type RosterEntry = Entity & { actor: number; tag: UnitTag };
import type { NameplateProjection } from "../domain/units/nameplates";
import { validCombatText, type CombatTextProjection } from "../domain/units/combat-text";
export type RoundResult = "won" | "lost" | "draw" | "time_limit_draw";
export type SpellSnapshot = {
  cooldown: number;
  maxCooldown: number;
  status: string;
  cost: number;
  range: number;
  castTime: number;
  power: number;
};
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
  /** Authoritative outcome from this player's side; absent from older clients. */
  roundResult?: RoundResult | null;
  /** Authoritative end-of-match per-seat totals in local actor order; null while running or in local practice. */
  scoreboard?: ScoreboardEntry[] | null;
  /** Total authoritative ticks in the round before a time-limit draw. */
  maxRoundTicks?: number;
  /** Authoritative simulation ticks per second. */
  tickRate?: number;
  /** Maximum primary resource pool; absent from older clients. */
  maxResource?: number;
  character: { status: string };
  /** Render warmup behind the loading screen; absent from older clients. */
  startup?: StartupState;
  camera: { yaw: number; pitch: number; distance: number };
  pulse: { damage: number; cooldown: number; range: number };
  /** Per-spell authoritative cooldowns, costs, ranges and readiness; absent from older clients. */
  spells?: Record<SpellKey, SpellSnapshot>;
};

export type StartupState = {
  phase: "assets" | "render" | "warming" | "ready";
  ready: boolean;
  pending: number;
  timedOut: boolean;
};

const STARTUP_PHASES = new Set(["assets", "render", "warming", "ready"]);
const ROUND_RESULTS = new Set<RoundResult>(["won", "lost", "draw", "time_limit_draw"]);
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

function validateCombatFields(
  unit: {
    resource?: unknown;
    gcdRemaining?: unknown;
    lockoutRemaining?: unknown;
    cast?: unknown;
  },
  maxResource: number | undefined,
): {
  resource?: number;
  maxResource?: number;
  gcdRemaining?: number;
  lockoutRemaining?: number;
  cast?: ActiveCast | null;
} {
  const out: {
    resource?: number;
    maxResource?: number;
    gcdRemaining?: number;
    lockoutRemaining?: number;
    cast?: ActiveCast | null;
  } = {};
  if (unit.resource !== undefined) {
    if (
      !isNonNegativeSafeInt(unit.resource) ||
      (maxResource !== undefined && unit.resource > maxResource)
    ) {
      throw new Error("Invalid unit resource");
    }
    out.resource = unit.resource;
    if (maxResource !== undefined) out.maxResource = maxResource;
  }
  if (unit.gcdRemaining !== undefined) {
    if (!isNonNegativeSafeInt(unit.gcdRemaining)) throw new Error("Invalid unit GCD");
    out.gcdRemaining = unit.gcdRemaining;
  }
  if (unit.lockoutRemaining !== undefined) {
    if (!isNonNegativeSafeInt(unit.lockoutRemaining)) throw new Error("Invalid unit lockout");
    out.lockoutRemaining = unit.lockoutRemaining;
  }
  if (unit.cast !== undefined) {
    if (unit.cast !== null && !isValidActiveCast(unit.cast)) {
      throw new Error("Invalid unit active cast");
    }
    out.cast = unit.cast;
  }
  return out;
}

const SPELL_KEYS: readonly SpellKey[] = ["pulse", "rift_bolt", "quell", "mend"];

function validateSpells(value: unknown): Record<SpellKey, SpellSnapshot> {
  if (!value || typeof value !== "object") throw new Error("Invalid spell metadata");
  const raw = value as Record<string, unknown>;
  const out = {} as Record<SpellKey, SpellSnapshot>;
  for (const key of SPELL_KEYS) {
    const entry = raw[key] as SpellSnapshot | undefined;
    if (
      !entry ||
      typeof entry !== "object" ||
      !isNonNegativeFinite(entry.cooldown) ||
      !isNonNegativeFinite(entry.maxCooldown) ||
      typeof entry.status !== "string" ||
      !isNonNegativeSafeInt(entry.cost) ||
      !isNonNegativeFinite(entry.range) ||
      !isNonNegativeFinite(entry.castTime) ||
      !isNonNegativeSafeInt(entry.power)
    ) {
      throw new Error("Invalid spell metadata");
    }
    out[key] = {
      cooldown: entry.cooldown,
      maxCooldown: entry.maxCooldown,
      status: entry.status,
      cost: entry.cost,
      range: entry.range,
      castTime: entry.castTime,
      power: entry.power,
    };
  }
  return out;
}

function spellCooldownEdgesDiffer(
  previous: Record<SpellKey, SpellSnapshot> | undefined,
  next: Record<SpellKey, SpellSnapshot> | undefined,
) {
  if (!previous || !next) return Boolean(previous) !== Boolean(next);
  for (const key of SPELL_KEYS) {
    const a = previous[key];
    const b = next[key];
    if (b.cooldown > a.cooldown || (b.cooldown === 0) !== (a.cooldown === 0)) return true;
  }
  return false;
}

function combatEdgesDiffer(
  previous: { cast?: ActiveCast | null; lockoutRemaining?: number } | undefined,
  next: { cast?: ActiveCast | null; lockoutRemaining?: number } | undefined,
) {
  return (
    Boolean(next?.cast) !== Boolean(previous?.cast) ||
    next?.cast?.spell !== previous?.cast?.spell ||
    (next?.lockoutRemaining ?? 0) > 0 !== (previous?.lockoutRemaining ?? 0) > 0
  );
}

export function needsImmediateDelivery(previous: Snapshot | null, next: Snapshot) {
  if (
    !previous ||
    next.tick < previous.tick ||
    next.teamSize !== previous.teamSize ||
    next.selectedActor !== previous.selectedActor ||
    next.won !== previous.won ||
    next.roundResult !== previous.roundResult ||
    Boolean(next.scoreboard) !== Boolean(previous.scoreboard) ||
    next.maxRoundTicks !== previous.maxRoundTicks ||
    next.tickRate !== previous.tickRate ||
    snapshotRemainingSeconds(next) !== snapshotRemainingSeconds(previous) ||
    next.startup?.ready !== previous.startup?.ready ||
    next.hoveredUnit !== previous.hoveredUnit ||
    next.cooldown > previous.cooldown ||
    (next.cooldown === 0) !== (previous.cooldown === 0) ||
    spellCooldownEdgesDiffer(previous.spells, next.spells) ||
    combatEdgesDiffer(previous.player, next.player) ||
    combatEdgesDiffer(previous.target, next.target)
  ) {
    return true;
  }
  if (previous.roster && next.roster && previous.roster.length === next.roster.length) {
    for (let i = 0; i < next.roster.length; i += 1) {
      if (combatEdgesDiffer(previous.roster[i], next.roster[i])) return true;
    }
  }
  return false;
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
    (r.roundResult !== undefined && r.roundResult !== null && !ROUND_RESULTS.has(r.roundResult)) ||
    (r.roundResult !== undefined && r.won !== (r.roundResult !== null)) ||
    (r.maxRoundTicks !== undefined &&
      (!Number.isSafeInteger(r.maxRoundTicks) || r.maxRoundTicks < 1)) ||
    (r.tickRate !== undefined && (!Number.isSafeInteger(r.tickRate) || r.tickRate < 1)) ||
    typeof r.feedback !== "string" ||
    typeof r.attackStatus !== "string"
  )
    throw new Error("Invalid engine snapshot flags");
  const maxHealth = (raw as { maxHealth?: number }).maxHealth;
  const maxResource = (raw as { maxResource?: unknown }).maxResource;
  if (
    maxResource !== undefined &&
    (typeof maxResource !== "number" || !Number.isSafeInteger(maxResource) || maxResource < 1)
  ) {
    throw new Error("Invalid maximum resource");
  }
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
      const tag = expectedActorTag(actor);
      if (
        unit.actor !== actor ||
        unit.tag !== tag ||
        !Number.isFinite(unit.health) ||
        unit.health < 0 ||
        unit.health > maxHealth
      )
        throw new Error("Invalid practice unit");
      const conn = (unit as { connected?: boolean }).connected;
      const combat = validateCombatFields(unit, maxResource);
      roster[actor] = {
        actor,
        tag: tag as UnitTag,
        health: unit.health,
        maxHealth,
        ...combat,
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
  let scoreboard: ScoreboardEntry[] | null | undefined;
  if (r.scoreboard !== undefined) {
    if (r.scoreboard === null) {
      scoreboard = null;
    } else {
      if (
        !r.won ||
        !Array.isArray(r.scoreboard) ||
        (r.scoreboard.length !== 2 && r.scoreboard.length !== 4 && r.scoreboard.length !== 6) ||
        (roster !== undefined && r.scoreboard.length !== roster.length)
      ) {
        throw new Error("Invalid scoreboard");
      }
      scoreboard = new Array<ScoreboardEntry>(r.scoreboard.length);
      for (let actor = 0; actor < r.scoreboard.length; actor += 1) {
        const entry = r.scoreboard[actor];
        const tag = expectedActorTag(actor);
        if (!isValidScoreboardEntry(entry, actor, tag)) {
          throw new Error("Invalid scoreboard entry");
        }
        scoreboard[actor] = {
          actor,
          tag,
          damageDone: entry.damageDone,
          damageTaken: entry.damageTaken,
          healingDone: entry.healingDone,
          healingReceived: entry.healingReceived,
          killingBlows: entry.killingBlows,
        };
      }
    }
  }
  const playerCombat = validateCombatFields(r.player, maxResource);
  const targetCombat = validateCombatFields(r.target, maxResource);
  const spells = r.spells !== undefined ? validateSpells(r.spells) : undefined;
  const opponent = roster?.[1];
  return {
    ...r,
    ...(maxResource !== undefined ? { maxResource } : {}),
    ...(spells !== undefined ? { spells } : {}),
    ...(scoreboard !== undefined ? { scoreboard } : {}),
    roster,
    networked,
    player: { ...r.player, ...playerCombat, maxHealth, id: "practice-player", name: "Player" },
    target: {
      ...r.target,
      ...targetCombat,
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
