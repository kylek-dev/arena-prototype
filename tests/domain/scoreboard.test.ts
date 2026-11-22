import { describe, expect, it } from "vitest";
import {
  buildMatchScoreboard,
  DEFAULT_SCOREBOARD_SORT,
  expectedActorTag,
  formatMatchDuration,
  isValidScoreboardEntry,
  SCOREBOARD_COLUMNS,
  scoreboardQueueMode,
  scoreboardTone,
  sortScoreboardRows,
  sumTeamTotals,
  toggleScoreboardSort,
  type ScoreboardEntry,
} from "../../web/src/domain/scoreboard";
import { teamSnapshot } from "../ui/fixtures";

function sample2v2Entries(): ScoreboardEntry[] {
  return [
    {
      actor: 0,
      tag: "Player",
      damageDone: 150,
      damageTaken: 60,
      healingDone: 30,
      healingReceived: 45,
      killingBlows: 1,
    },
    {
      actor: 1,
      tag: "Opponent1",
      damageDone: 90,
      damageTaken: 180,
      healingDone: 0,
      healingReceived: 25,
      killingBlows: 0,
    },
    {
      actor: 2,
      tag: "Team1",
      damageDone: 150,
      damageTaken: 85,
      healingDone: 60,
      healingReceived: 15,
      killingBlows: 1,
    },
    {
      actor: 3,
      tag: "Opponent2",
      damageDone: 55,
      damageTaken: 120,
      healingDone: 25,
      healingReceived: 0,
      killingBlows: 0,
    },
  ];
}

describe("scoreboard domain", () => {
  it("groups players by team parity, highlights the local player, and computes totals", () => {
    const base = teamSnapshot(2);
    const roster = base.roster?.map((unit) =>
      unit.actor === 3 ? { ...unit, disconnected: true } : unit,
    );
    const board = buildMatchScoreboard({
      tick: 3675,
      tickRate: 30,
      roundResult: "won",
      roster,
      scoreboard: sample2v2Entries(),
    });
    expect(board).not.toBeNull();
    expect(board?.result).toBe("won");
    expect(board?.resultLabel).toBe("Victory");
    expect(board?.durationTicks).toBe(3675);
    expect(board?.tickRate).toBe(30);
    expect(board?.allies.map((r) => [r.actor, r.name, r.isLocalPlayer, r.disconnected])).toEqual([
      [0, "Player", true, false],
      [2, "Teammate 1", false, false],
    ]);
    expect(board?.opponents.map((r) => [r.actor, r.name, r.isLocalPlayer, r.disconnected])).toEqual(
      [
        [1, "Enemy Bot", false, false],
        [3, "Enemy Bot 2", false, true],
      ],
    );
    expect(board?.allyTotals).toEqual({
      damageDone: 300,
      damageTaken: 145,
      healingDone: 90,
      healingReceived: 60,
      killingBlows: 2,
    });
    expect(board?.opponentTotals).toEqual({
      damageDone: 145,
      damageTaken: 300,
      healingDone: 25,
      healingReceived: 25,
      killingBlows: 0,
    });
  });

  it("uses deterministic fallback names when roster is omitted and handles time-limit draws", () => {
    const board = buildMatchScoreboard({
      tick: 36_000,
      tickRate: 30,
      roundResult: "time_limit_draw",
      scoreboard: sample2v2Entries(),
    });
    expect(board?.resultLabel).toBe("Time's up · draw");
    expect(board?.allies.map((r) => r.name)).toEqual(["Player", "Teammate 1"]);
    expect(board?.opponents.map((r) => r.name)).toEqual(["Opponent 1", "Opponent 2"]);
  });

  it("returns null while the round is running or scoreboard data is missing/invalid", () => {
    expect(
      buildMatchScoreboard({
        tick: 100,
        roundResult: null,
        scoreboard: sample2v2Entries(),
      }),
    ).toBeNull();
    expect(
      buildMatchScoreboard({
        tick: 100,
        roundResult: "won",
        scoreboard: null,
      }),
    ).toBeNull();
    expect(
      buildMatchScoreboard({
        tick: 100,
        roundResult: "won",
        scoreboard: sample2v2Entries().slice(0, 3),
      }),
    ).toBeNull();
  });

  it("sorts rows non-mutatingly with deterministic tie-breaking and toggles sort state", () => {
    const board = buildMatchScoreboard({
      tick: 900,
      roundResult: "won",
      scoreboard: sample2v2Entries(),
    })!;

    // Allies both have damageDone=150 and killingBlows=1; tie-breaker picks higher healingDone (actor 2).
    const beforeActors = board.allies.map((r) => r.actor);
    const byDamageDesc = sortScoreboardRows(board.allies, DEFAULT_SCOREBOARD_SORT);
    expect(byDamageDesc.map((r) => r.actor)).toEqual([2, 0]);
    expect(board.allies.map((r) => r.actor)).toEqual(beforeActors);

    const byDamageTakenAsc = sortScoreboardRows(board.allies, {
      key: "damageTaken",
      direction: "asc",
    });
    expect(byDamageTakenAsc.map((r) => r.actor)).toEqual([0, 2]);

    const byHealingDesc = sortScoreboardRows(board.opponents, {
      key: "healingDone",
      direction: "desc",
    });
    expect(byHealingDesc.map((r) => r.actor)).toEqual([3, 1]);

    // Toggle logic: same column flips direction; new column resets to desc.
    const flipped = toggleScoreboardSort(DEFAULT_SCOREBOARD_SORT, "damageDone");
    expect(flipped).toEqual({ key: "damageDone", direction: "asc" });
    expect(toggleScoreboardSort(flipped, "damageDone")).toEqual({
      key: "damageDone",
      direction: "desc",
    });
    expect(toggleScoreboardSort(flipped, "healingDone")).toEqual({
      key: "healingDone",
      direction: "desc",
    });
    expect(SCOREBOARD_COLUMNS.map((c) => c.key)).toEqual([
      "damageDone",
      "damageTaken",
      "healingDone",
      "healingReceived",
      "killingBlows",
    ]);
  });

  it("validates individual entries, computes empty totals, and formats match durations", () => {
    expect(expectedActorTag(0)).toBe("Player");
    expect(expectedActorTag(1)).toBe("Opponent1");
    expect(expectedActorTag(2)).toBe("Team1");
    expect(expectedActorTag(5)).toBe("Opponent3");

    const entry = sample2v2Entries()[0];
    expect(isValidScoreboardEntry(entry, 0, "Player")).toBe(true);
    expect(isValidScoreboardEntry(entry, 1, "Opponent1")).toBe(false);
    expect(isValidScoreboardEntry({ ...entry, damageDone: -1 }, 0, "Player")).toBe(false);
    expect(isValidScoreboardEntry({ ...entry, healingReceived: 1.2 }, 0, "Player")).toBe(false);
    expect(isValidScoreboardEntry(null, 0, "Player")).toBe(false);

    expect(sumTeamTotals([])).toEqual({
      damageDone: 0,
      damageTaken: 0,
      healingDone: 0,
      healingReceived: 0,
      killingBlows: 0,
    });

    expect(formatMatchDuration(0, 30)).toBe("00:00");
    expect(formatMatchDuration(29, 30)).toBe("00:00");
    expect(formatMatchDuration(30, 30)).toBe("00:01");
    expect(formatMatchDuration(3675, 30)).toBe("02:02");
    expect(formatMatchDuration(36_000, 30)).toBe("20:00");
    expect(formatMatchDuration(-5, 30)).toBe("00:00");
    expect(formatMatchDuration(100, 0)).toBe("00:00");

    const board = buildMatchScoreboard({
      tick: 900,
      roundResult: "won",
      scoreboard: sample2v2Entries(),
    })!;
    expect(scoreboardQueueMode(board)).toBe("2v2");
    expect(
      scoreboardQueueMode({
        ...board,
        allies: board.allies.slice(0, 1),
        opponents: board.opponents.slice(0, 1),
      }),
    ).toBe("1v1");
    expect(
      scoreboardQueueMode({
        ...board,
        allies: [...board.allies, board.allies[0]],
        opponents: [...board.opponents, board.opponents[0]],
      }),
    ).toBe("3v3");
    expect(
      scoreboardQueueMode({
        ...board,
        allies: board.allies.slice(0, 1),
        opponents: board.opponents,
      }),
    ).toBeNull();
    expect(scoreboardTone("won")).toBe("victory");
    expect(scoreboardTone("lost")).toBe("defeat");
    expect(scoreboardTone("draw")).toBe("draw");
    expect(scoreboardTone("time_limit_draw")).toBe("time-limit");
  });
});
