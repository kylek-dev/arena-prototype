// Pure end-of-match scoreboard domain helpers (no DOM, React, or engine dependencies).
import type { RosterEntry, RoundResult } from "../bridge/snapshot";
import type { QueueMode } from "./matchmaking";
import {
  formatRoundClock,
  roundResultLabel,
  roundResultTone,
  type RoundResultTone,
} from "./round-clock";
import type { UnitTag } from "./units/tags";

/** Authoritative per-seat totals published in the engine snapshot (in local actor order). */
export type ScoreboardEntry = {
  actor: number;
  tag: UnitTag;
  damageDone: number;
  damageTaken: number;
  healingDone: number;
  healingReceived: number;
  killingBlows: number;
};

export type ScoreboardTeam = "allies" | "opponents";

export type ScoreboardRow = ScoreboardEntry & {
  name: string;
  team: ScoreboardTeam;
  isLocalPlayer: boolean;
  disconnected: boolean;
  /** The server's admin tag for this fighter's player. */
  admin: boolean;
};

export type ScoreboardSortKey =
  | "damageDone"
  | "damageTaken"
  | "healingDone"
  | "healingReceived"
  | "killingBlows";

export type SortDirection = "desc" | "asc";

export type ScoreboardSort = {
  key: ScoreboardSortKey;
  direction: SortDirection;
};

export type ScoreboardColumn = {
  key: ScoreboardSortKey;
  label: string;
  shortLabel: string;
};

export type TeamTotals = {
  damageDone: number;
  damageTaken: number;
  healingDone: number;
  healingReceived: number;
  killingBlows: number;
};

/** Immutable post-match summary retained locally after the server closes the match socket. */
export type MatchScoreboard = {
  result: RoundResult;
  resultLabel: string;
  durationTicks: number;
  tickRate: number;
  spectating?: boolean;
  allies: ScoreboardRow[];
  opponents: ScoreboardRow[];
  allyTotals: TeamTotals;
  opponentTotals: TeamTotals;
};

export const DEFAULT_SCOREBOARD_SORT: ScoreboardSort = {
  key: "damageDone",
  direction: "desc",
};

export const SCOREBOARD_COLUMNS: readonly ScoreboardColumn[] = [
  { key: "damageDone", label: "Damage Done", shortLabel: "Dmg" },
  { key: "damageTaken", label: "Damage Taken", shortLabel: "Taken" },
  { key: "healingDone", label: "Healing Done", shortLabel: "Heal" },
  { key: "healingReceived", label: "Healing Received", shortLabel: "Recv" },
  { key: "killingBlows", label: "Killing Blows", shortLabel: "KB" },
] as const;

const VALID_ROSTER_SIZES = new Set([2, 4, 6]);

export function isNonNegativeSafeInt(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}

/** Canonical local unit tag for local actor index `0..6`. */
export function expectedActorTag(actor: number): UnitTag {
  if (actor === 0) return "Player";
  if (actor % 2 === 0) return `Team${actor / 2}` as UnitTag;
  return `Opponent${Math.floor(actor / 2) + 1}` as UnitTag;
}

function fallbackActorName(actor: number, spectating = false): string {
  if (spectating) {
    return actor % 2 === 0 ? `Team A ${actor / 2 + 1}` : `Team B ${Math.floor(actor / 2) + 1}`;
  }
  if (actor === 0) return "Player";
  if (actor % 2 === 0) return `Teammate ${actor / 2}`;
  return `Opponent ${Math.floor(actor / 2) + 1}`;
}

export function isValidScoreboardEntry(
  value: unknown,
  expectedActor: number,
  expectedTag: UnitTag,
): value is ScoreboardEntry {
  if (!value || typeof value !== "object") return false;
  const e = value as ScoreboardEntry;
  return (
    e.actor === expectedActor &&
    e.tag === expectedTag &&
    isNonNegativeSafeInt(e.damageDone) &&
    isNonNegativeSafeInt(e.damageTaken) &&
    isNonNegativeSafeInt(e.healingDone) &&
    isNonNegativeSafeInt(e.healingReceived) &&
    isNonNegativeSafeInt(e.killingBlows)
  );
}

export function sumTeamTotals(rows: readonly ScoreboardEntry[]): TeamTotals {
  let damageDone = 0;
  let damageTaken = 0;
  let healingDone = 0;
  let healingReceived = 0;
  let killingBlows = 0;
  for (const row of rows) {
    damageDone += row.damageDone;
    damageTaken += row.damageTaken;
    healingDone += row.healingDone;
    healingReceived += row.healingReceived;
    killingBlows += row.killingBlows;
  }
  return { damageDone, damageTaken, healingDone, healingReceived, killingBlows };
}

/** Returns a new sorted array without mutating `rows`; deterministic tie-breaking ends at `actor`. */
export function sortScoreboardRows(
  rows: readonly ScoreboardRow[],
  sort: ScoreboardSort = DEFAULT_SCOREBOARD_SORT,
): ScoreboardRow[] {
  const dir = sort.direction === "asc" ? 1 : -1;
  return [...rows].sort((a, b) => {
    const primary = (a[sort.key] - b[sort.key]) * dir;
    if (primary !== 0) return primary;
    if (sort.key !== "killingBlows" && b.killingBlows !== a.killingBlows) {
      return b.killingBlows - a.killingBlows;
    }
    if (sort.key !== "damageDone" && b.damageDone !== a.damageDone) {
      return b.damageDone - a.damageDone;
    }
    if (sort.key !== "healingDone" && b.healingDone !== a.healingDone) {
      return b.healingDone - a.healingDone;
    }
    return a.actor - b.actor;
  });
}

/** Clicking a new column sorts descending; clicking the active column flips direction. */
export function toggleScoreboardSort(
  current: ScoreboardSort,
  key: ScoreboardSortKey,
): ScoreboardSort {
  if (current.key === key) {
    return { key, direction: current.direction === "desc" ? "asc" : "desc" };
  }
  return { key, direction: "desc" };
}

/** Formats elapsed match duration in whole seconds as `MM:SS`. */
export function formatMatchDuration(durationTicks: number, tickRate = 30): string {
  if (
    !Number.isFinite(durationTicks) ||
    !Number.isFinite(tickRate) ||
    durationTicks <= 0 ||
    tickRate <= 0
  ) {
    return "00:00";
  }
  return formatRoundClock(Math.floor(durationTicks / tickRate));
}

export type ScoreboardSource = {
  tick: number;
  tickRate?: number;
  spectating?: boolean;
  roundResult?: RoundResult | null;
  roster?: readonly RosterEntry[];
  scoreboard?: readonly ScoreboardEntry[] | null;
};

/** Builds a grouped, immutable end-of-match scoreboard from a finished engine snapshot. */
export function buildMatchScoreboard(source: ScoreboardSource): MatchScoreboard | null {
  const result = source.roundResult;
  const entries = source.scoreboard;
  if (!result || !entries || !VALID_ROSTER_SIZES.has(entries.length)) {
    return null;
  }
  const spectating = Boolean(source.spectating);
  const resultLabel = roundResultLabel(result, spectating);
  if (!resultLabel) return null;

  const allies: ScoreboardRow[] = [];
  const opponents: ScoreboardRow[] = [];
  for (const entry of entries) {
    const rosterUnit = source.roster?.find((unit) => unit.actor === entry.actor);
    const team: ScoreboardTeam = entry.actor % 2 === 0 ? "allies" : "opponents";
    const row: ScoreboardRow = {
      actor: entry.actor,
      tag: entry.tag,
      damageDone: entry.damageDone,
      damageTaken: entry.damageTaken,
      healingDone: entry.healingDone,
      healingReceived: entry.healingReceived,
      killingBlows: entry.killingBlows,
      name: rosterUnit?.name ?? fallbackActorName(entry.actor, spectating),
      team,
      isLocalPlayer: !spectating && entry.actor === 0,
      disconnected: Boolean(rosterUnit?.disconnected),
      admin: Boolean(rosterUnit?.admin),
    };
    if (team === "allies") allies.push(row);
    else opponents.push(row);
  }
  const tickRate =
    typeof source.tickRate === "number" &&
    Number.isSafeInteger(source.tickRate) &&
    source.tickRate > 0
      ? source.tickRate
      : 30;
  const durationTicks =
    typeof source.tick === "number" && Number.isFinite(source.tick) && source.tick >= 0
      ? Math.floor(source.tick)
      : 0;

  return {
    result,
    resultLabel,
    durationTicks,
    tickRate,
    ...(spectating ? { spectating: true } : {}),
    allies,
    opponents,
    allyTotals: sumTeamTotals(allies),
    opponentTotals: sumTeamTotals(opponents),
  };
}

/** Infers the matchmaking queue mode (`1v1`, `2v2`, `3v3`) from the scoreboard's team size. */
export function scoreboardQueueMode(board: MatchScoreboard): QueueMode | null {
  if (board.allies.length === 1 && board.opponents.length === 1) return "1v1";
  if (board.allies.length === 2 && board.opponents.length === 2) return "2v2";
  if (board.allies.length === 3 && board.opponents.length === 3) return "3v3";
  return null;
}

export type ScoreboardTone = RoundResultTone;

export function scoreboardTone(result: RoundResult): ScoreboardTone {
  return roundResultTone(result);
}
