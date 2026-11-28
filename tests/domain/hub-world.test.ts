// Hub worlds through the engine boundary and the page's match logic
// (ADR 0057 slice 5).
import { describe, expect, it } from "vitest";
import { adaptSnapshot } from "../../web/src/bridge/snapshot";
import { withHandles, withTags } from "../../web/src/domain/handles";
import { inMatch, matchMessage } from "../../web/src/domain/network";
import { resolveUnit } from "../../web/src/domain/units/tags";
import { snapshot } from "../ui/fixtures";

const unit = (actor: number, tag: string, extra: object = {}) => ({
  actor,
  tag,
  health: 100,
  ...extra,
});

/** A world as the engine publishes it: actors 0 (this player), 1–4 the
 * training enemies, 6 and 9 other players; 5, 7 and 8 vacant (left out). */
function world(overrides: object = {}) {
  return {
    ...snapshot(),
    hub: true,
    maxHealth: 100,
    teamSize: 3,
    selectedActor: 9,
    selected: true,
    selectedPlayer: false,
    roster: [
      unit(0, "Player", { connected: true }),
      unit(1, "Opponent1", { connected: false }),
      unit(2, "Opponent2", { connected: false }),
      unit(3, "Opponent3", { connected: false }),
      unit(4, "Opponent4", { connected: false }),
      unit(6, "Team1", { connected: true }),
      unit(9, "Team2", { connected: false }),
    ],
    hoveredUnit: "Opponent4",
    ...overrides,
  };
}

describe("hub worlds at the engine boundary", () => {
  it("accepts a world's roster, with names for players and training enemies", () => {
    const s = adaptSnapshot(world());
    expect(s.hub).toBe(true);
    expect(s.roster?.map((u) => [u.actor, u.tag, u.name])).toEqual([
      [0, "Player", "Player"],
      [1, "Opponent1", "Training Dummy 1"],
      [2, "Opponent2", "Training Dummy 2"],
      [3, "Opponent3", "Training Dummy 3"],
      [4, "Opponent4", "Training Dummy 4"],
      [6, "Team1", "Player 1"],
      [9, "Team2", "Player 2"],
    ]);
    // Only players are ever shown as disconnected.
    expect(s.roster?.find((u) => u.actor === 9)?.disconnected).toBe(true);
    expect(s.roster?.find((u) => u.actor === 1)?.disconnected).toBeUndefined();
    // The fourth training enemy can be hovered (a match has only three).
    expect(s.hoveredUnit).toBe("Opponent4");
    expect(
      resolveUnit("Opponent4", {
        player: s.player,
        target: null,
        focus: null,
        opponents: s.roster!.filter((u) => u.tag.startsWith("Opponent")),
        team: [],
      })?.id,
    ).toBe("world-actor-4");
  });

  it("refuses rosters a world could not have", () => {
    const bad = (overrides: object) => () => adaptSnapshot(world(overrides));
    expect(bad({ roster: [unit(1, "Opponent1")] })).toThrow();
    expect(bad({ roster: [unit(0, "Player"), unit(1, "Team1"), unit(1, "Team2")] })).toThrow();
    expect(bad({ roster: [unit(0, "Player"), unit(2, "Team1"), unit(1, "Team2")] })).toThrow();
    expect(bad({ roster: [unit(0, "Player"), unit(1, "Team1"), unit(2, "Team1")] })).toThrow();
    expect(bad({ roster: [unit(0, "Player"), unit(44, "Team1")] })).toThrow();
    expect(bad({ roster: [unit(0, "Player"), unit(1, "Team44")] })).toThrow();
    expect(bad({ hoveredUnit: "Team44" })).toThrow();
    // The largest world's tags are fine.
    expect(() =>
      adaptSnapshot(
        world({
          roster: [unit(0, "Player"), unit(43, "Team39")],
          selectedActor: null,
          selected: false,
          hoveredUnit: "Team39",
        }),
      ),
    ).not.toThrow();
    expect(bad({ selectedActor: 5 })).toThrow(); // a vacant slot
    expect(bad({ spectating: true })).toThrow();
    // A match still refuses a fourth opponent.
    expect(() =>
      adaptSnapshot({ ...world(), hub: false, roster: undefined, hoveredUnit: "Opponent4" }),
    ).toThrow();
  });

  it("names units from per-actor relays though vacant slots are left out", () => {
    const s = adaptSnapshot(world());
    const handles: (string | null)[] = Array(10).fill(null);
    handles[0] = "Rook#0042";
    handles[9] = "Ash#1000";
    const tags = Array.from({ length: 10 }, (_, actor) => ({ admin: actor === 9 }));
    const named = withTags(withHandles(s, handles), tags);
    expect(named.player.name).toBe("Rook#0042");
    const ash = named.roster?.find((u) => u.actor === 9);
    expect(ash?.name).toBe("Ash#1000");
    expect(ash?.admin).toBe(true);
    expect(named.roster?.find((u) => u.actor === 6)?.name).toBe("Player 1");
  });
});

describe("a hub world is not a match", () => {
  it("leaves queueing and the rest of the page available", () => {
    const open = { phase: "open" as const, attempt: 0, ended: null, status: null };
    expect(inMatch(open)).toBe(true);
    expect(inMatch({ ...open, world: true })).toBe(false);
    expect(inMatch({ ...open, phase: "idle" })).toBe(false);
    expect(inMatch({ ...open, phase: "ended" })).toBe(false);
  });

  it("speaks of the hub, and offers a remembered match once out of it", () => {
    const base = { attempt: 0, ended: null, status: null, world: true };
    expect(matchMessage({ ...base, phase: "connecting" }, false)?.text).toBe("Entering the hub…");
    expect(matchMessage({ ...base, phase: "reconnecting", attempt: 2 }, false)?.text).toMatch(
      /hub lost/,
    );
    // Idle after leaving the hub: the remembered match is offered as usual.
    expect(matchMessage({ ...base, phase: "idle" }, true)?.action?.kind).toBe("rejoin");
  });
});
