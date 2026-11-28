import { describe, expect, it } from "vitest";
import { adaptSnapshot, needsImmediateDelivery } from "../../web/src/bridge/snapshot";
import { availability, canActivate } from "../../web/src/domain/abilities/catalog";
import { snapshot, teamSnapshot } from "../ui/fixtures";
import { defaultConfig, validateConfig } from "../../web/src/domain/preferences/schema";

describe("engine snapshot boundary", () => {
  it("validates roster identity, counts, selection and health", () => {
    const valid = { ...teamSnapshot(3), maxHealth: 100 };
    expect(adaptSnapshot(valid).roster).toHaveLength(6);
    for (const invalid of [
      { ...valid, teamSize: 4 },
      { ...valid, selectedActor: 6 },
      { ...valid, selected: false },
      { ...valid, roster: valid.roster?.slice(1) },
      { ...valid, roster: valid.roster?.map((unit) => ({ ...unit, tag: "Player" })) },
      { ...valid, roster: valid.roster?.map((unit) => ({ ...unit, health: 101 })) },
    ])
      expect(() => adaptSnapshot(invalid)).toThrow();
  });

  it("names each cast ending's unit by entity id and rejects malformed endings", () => {
    const team = { ...teamSnapshot(2), maxHealth: 100 };
    const end = { id: 4, actor: 3, spell: "mend", outcome: "interrupted" };
    expect(adaptSnapshot({ ...team, castEnds: [end] }).castEnds).toEqual([
      { id: 4, unit: "practice-actor-3", spell: "mend", outcome: "interrupted" },
    ]);
    // Without a roster, actors 0 and 1 are the player and the opponent.
    const practice = { ...snapshot(), maxHealth: 100 };
    expect(
      adaptSnapshot({
        ...practice,
        castEnds: [
          { id: 1, actor: 0, spell: "rift_bolt", outcome: "completed" },
          { id: 2, actor: 1, spell: "mend", outcome: "cancelled" },
        ],
      }).castEnds?.map((e) => e.unit),
    ).toEqual(["practice-player", "practice-opponent"]);
    expect(adaptSnapshot(practice).castEnds).toBeUndefined();
    // Regression: an instant interrupt (Quell) reports completing; it has no
    // cast bar and must never fail the snapshot.
    expect(
      adaptSnapshot({
        ...team,
        castEnds: [{ id: 5, actor: 0, spell: "quell", outcome: "completed" }, end],
      }).castEnds?.map((e) => e.spell),
    ).toEqual(["mend"]);
    for (const castEnds of [
      {},
      [{ ...end, actor: 9 }],
      [{ ...end, id: 0 }],
      [end, end],
      [{ ...end, spell: "fireball" }],
      [{ ...end, outcome: "fizzled" }],
      Array.from({ length: 33 }, (_, i) => ({ ...end, id: i + 1 })),
    ])
      expect(() => adaptSnapshot({ ...team, castEnds })).toThrow();
  });

  it("adds left team placement to old profiles without moving existing frames", () => {
    const old = defaultConfig();
    const { team: _team, ...frames } = old.frames;
    const migrated = validateConfig({ ...old, frames });
    expect(migrated.frames.team.x).toBe(0.03);
    expect(migrated.frames.opponents).toEqual(old.frames.opponents);
    expect(validateConfig(migrated)).toEqual(migrated);
  });
  it("adapts the two-entity practice ABI without inventing combat data", () => {
    const raw = { ...snapshot(), maxHealth: 150 };
    const adapted = adaptSnapshot(raw);
    expect(adapted.target.id).toBe("practice-opponent");
    expect(adapted.player.maxHealth).toBe(150);
    expect(adapted.pulse).toEqual(raw.pulse);
    expect(adapted.cooldown).toBe(raw.cooldown);
  });

  it("validates round clock metadata and roundResult consistency", () => {
    const running = adaptSnapshot({
      ...snapshot(),
      maxHealth: 100,
      maxRoundTicks: 36_000,
      tickRate: 30,
      roundResult: null,
    });
    expect(running.maxRoundTicks).toBe(36_000);
    expect(running.tickRate).toBe(30);
    expect(running.roundResult).toBeNull();
    const timedOut = adaptSnapshot({
      ...snapshot({ won: true }),
      maxHealth: 100,
      maxRoundTicks: 36_000,
      tickRate: 30,
      roundResult: "time_limit_draw",
    });
    expect(timedOut.roundResult).toBe("time_limit_draw");
    expect(needsImmediateDelivery(running, timedOut)).toBe(true);
    expect(needsImmediateDelivery(running, { ...running, maxRoundTicks: 900 })).toBe(true);
    expect(needsImmediateDelivery(running, { ...running, tickRate: 60 })).toBe(true);
    // Whole-second countdown transitions bypass HUD throttling; sub-second ticks stay throttled.
    expect(needsImmediateDelivery({ ...running, tick: 29 }, { ...running, tick: 30 })).toBe(true);
    expect(needsImmediateDelivery({ ...running, tick: 30 }, { ...running, tick: 31 })).toBe(false);
    expect(needsImmediateDelivery({ ...running, tick: 35_999 }, { ...running, tick: 36_000 })).toBe(
      true,
    );
    // Older snapshots omitting round-clock metadata stay throttled across tick boundaries.
    const legacy = adaptSnapshot({ ...snapshot({ tick: 29 }), maxHealth: 100 });
    expect(needsImmediateDelivery(legacy, { ...legacy, tick: 30 })).toBe(false);
    for (const invalid of [
      { ...snapshot(), maxHealth: 100, maxRoundTicks: 0 },
      { ...snapshot(), maxHealth: 100, maxRoundTicks: 1.5 },
      { ...snapshot(), maxHealth: 100, maxRoundTicks: 1e30 },
      { ...snapshot(), maxHealth: 100, tickRate: 0 },
      { ...snapshot(), maxHealth: 100, tickRate: 1e30 },
      { ...snapshot(), maxHealth: 100, roundResult: "forfeit" },
      { ...snapshot({ won: false }), maxHealth: 100, roundResult: "time_limit_draw" },
      { ...snapshot({ won: true }), maxHealth: 100, roundResult: null },
    ]) {
      expect(() => adaptSnapshot(invalid)).toThrow();
    }
  });

  it("validates authoritative end-of-match scoreboard entries and immediate delivery", () => {
    const base = { ...teamSnapshot(1), maxHealth: 100 };
    const running = adaptSnapshot({ ...base, won: false, roundResult: null, scoreboard: null });
    expect(running.scoreboard).toBeNull();

    const validScores = [
      {
        actor: 0,
        tag: "Player",
        damageDone: 125,
        damageTaken: 50,
        healingDone: 30,
        healingReceived: 30,
        killingBlows: 1,
      },
      {
        actor: 1,
        tag: "Opponent1",
        damageDone: 50,
        damageTaken: 125,
        healingDone: 0,
        healingReceived: 0,
        killingBlows: 0,
      },
    ];
    const finished = adaptSnapshot({
      ...base,
      won: true,
      roundResult: "won",
      scoreboard: validScores,
    });
    expect(finished.scoreboard).toEqual(validScores);
    expect(needsImmediateDelivery(running, finished)).toBe(true);
    const finishedNoBoard = adaptSnapshot({
      ...base,
      won: true,
      roundResult: "won",
      scoreboard: null,
    });
    expect(needsImmediateDelivery(finishedNoBoard, finished)).toBe(true);

    for (const invalid of [
      // Scoreboard present while round is not finished.
      { ...base, won: false, roundResult: null, scoreboard: validScores },
      // Wrong length or mismatch against roster.
      { ...base, won: true, roundResult: "won", scoreboard: [] },
      { ...base, won: true, roundResult: "won", scoreboard: [validScores[0]] },
      {
        ...base,
        won: true,
        roundResult: "won",
        scoreboard: [...validScores, ...validScores],
      },
      // Wrong actor index or unit tag.
      {
        ...base,
        won: true,
        roundResult: "won",
        scoreboard: [validScores[0], { ...validScores[1], actor: 2 }],
      },
      {
        ...base,
        won: true,
        roundResult: "won",
        scoreboard: [validScores[0], { ...validScores[1], tag: "Team1" }],
      },
      // Negative, float, or unsafe integer stats.
      {
        ...base,
        won: true,
        roundResult: "won",
        scoreboard: [{ ...validScores[0], damageDone: -1 }, validScores[1]],
      },
      {
        ...base,
        won: true,
        roundResult: "won",
        scoreboard: [{ ...validScores[0], healingDone: 12.5 }, validScores[1]],
      },
      {
        ...base,
        won: true,
        roundResult: "won",
        scoreboard: [{ ...validScores[0], damageTaken: 1e30 }, validScores[1]],
      },
      {
        ...base,
        won: true,
        roundResult: "won",
        scoreboard: [{ ...validScores[0], killingBlows: Number.NaN }, validScores[1]],
      },
    ]) {
      expect(() => adaptSnapshot(invalid)).toThrow();
    }
  });

  it("validates authoritative resources, GCD, lockout, and active cast metadata", () => {
    const base = {
      ...teamSnapshot(1),
      maxHealth: 100,
      maxResource: 100,
    };
    const idle = adaptSnapshot({
      ...base,
      player: { ...base.player, resource: 100, gcdRemaining: 0, lockoutRemaining: 0, cast: null },
      target: { ...base.target, resource: 65, gcdRemaining: 12, lockoutRemaining: 0, cast: null },
      roster: base.roster?.map((unit, idx) => ({
        ...unit,
        resource: idx === 0 ? 100 : 65,
        gcdRemaining: idx === 0 ? 0 : 12,
        lockoutRemaining: 0,
        cast: null,
      })),
    });
    expect(idle.maxResource).toBe(100);
    expect(idle.player.resource).toBe(100);
    expect(idle.player.maxResource).toBe(100);
    expect(idle.target.resource).toBe(65);
    expect(idle.roster?.[1].gcdRemaining).toBe(12);

    const casting = adaptSnapshot({
      ...base,
      player: {
        ...base.player,
        resource: 65,
        gcdRemaining: 28,
        lockoutRemaining: 0,
        cast: { spell: "rift_bolt", remainingTicks: 44, totalTicks: 45 },
      },
      target: { ...base.target, resource: 100, gcdRemaining: 0, lockoutRemaining: 0, cast: null },
    });
    expect(casting.player.cast).toEqual({
      spell: "rift_bolt",
      remainingTicks: 44,
      totalTicks: 45,
    });
    // The cast buffer's held spell: a known spell key or null.
    const held = (buffered: unknown) =>
      adaptSnapshot({ ...base, player: { ...base.player, buffered } });
    expect(held("pulse").player.buffered).toBe("pulse");
    expect(held(null).player.buffered).toBeNull();
    expect(() => held("fireball")).toThrow("Invalid buffered spell");
    expect(needsImmediateDelivery(idle, casting)).toBe(true);
    const enemyLocked = adaptSnapshot({
      ...base,
      player: { ...base.player, resource: 100, gcdRemaining: 0, lockoutRemaining: 0, cast: null },
      target: { ...base.target, resource: 65, gcdRemaining: 12, lockoutRemaining: 60, cast: null },
      roster: base.roster?.map((unit, idx) => ({
        ...unit,
        resource: idx === 0 ? 100 : 65,
        gcdRemaining: idx === 0 ? 0 : 12,
        lockoutRemaining: idx === 1 ? 60 : 0,
        cast: null,
      })),
    });
    expect(needsImmediateDelivery(idle, enemyLocked)).toBe(true);

    for (const invalid of [
      { ...base, maxResource: 0 },
      { ...base, maxResource: 1e30 },
      { ...base, player: { ...base.player, resource: 101 } },
      { ...base, player: { ...base.player, resource: -1 } },
      { ...base, player: { ...base.player, gcdRemaining: 1.5 } },
      { ...base, player: { ...base.player, lockoutRemaining: -2 } },
      {
        ...base,
        player: { ...base.player, cast: { spell: "rift_bolt", remainingTicks: 0, totalTicks: 45 } },
      },
      {
        ...base,
        player: {
          ...base.player,
          cast: { spell: "rift_bolt", remainingTicks: 46, totalTicks: 45 },
        },
      },
      {
        ...base,
        player: { ...base.player, cast: { spell: "invalid", remainingTicks: 10, totalTicks: 45 } },
      },
    ]) {
      expect(() => adaptSnapshot(invalid)).toThrow();
    }
  });

  it.each([
    null,
    { ...snapshot(), maxHealth: 0 },
    { ...snapshot(), maxHealth: 100, pulse: undefined },
    { ...snapshot(), maxHealth: 100, cooldown: NaN },
    { ...snapshot(), maxHealth: 100, character: undefined },
    { ...snapshot(), maxHealth: 100, camera: undefined },
    { ...snapshot(), maxHealth: 100, hoveredUnit: "Team4" },
    {
      ...snapshot(),
      maxHealth: 100,
      startup: { phase: "warming", ready: true, pending: 0, timedOut: false },
    },
    {
      ...snapshot(),
      maxHealth: 100,
      startup: { phase: "later", ready: false, pending: 0, timedOut: false },
    },
  ])("rejects incomplete engine data: %j", (raw) => {
    expect(() => adaptSnapshot(raw)).toThrow();
  });

  it("has explicit availability for every unsupported activation state", () => {
    expect(availability(null)).toBe("Loading arena");
    expect(availability(snapshot({ selected: false }))).toBe("Select a target");
    expect(availability(snapshot({ won: true }))).toBe("Round complete");
    expect(availability(snapshot({ cooldown: 0.6 }))).toContain("Recharging");
    expect(canActivate(snapshot({ attackStatus: "Out of range" }))).toBe(false);
    expect(canActivate(snapshot({ attackStatus: "Line of sight blocked" }))).toBe(false);
    expect(canActivate(snapshot())).toBe(true);
  });
});
