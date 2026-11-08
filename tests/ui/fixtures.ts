import type { Snapshot } from "../../web/src/bridge/snapshot";
import { adaptSnapshot } from "../../web/src/bridge/snapshot";

export function teamSnapshot(size: 1 | 2 | 3, selectedActor: number | null = 1): Snapshot {
  return adaptSnapshot({
    ...snapshot(),
    maxHealth: 100,
    teamSize: size,
    selectedActor,
    selected: selectedActor !== null,
    selectedPlayer: selectedActor === 0,
    roster: Array.from({ length: size * 2 }, (_, actor) => ({
      actor,
      tag:
        actor === 0
          ? "Player"
          : actor % 2 === 0
            ? `Team${actor / 2}`
            : `Opponent${Math.floor(actor / 2) + 1}`,
      health: actor === 3 ? 50 : 100,
    })),
  });
}

export function snapshot(overrides: Partial<Snapshot> = {}): Snapshot {
  return {
    tick: 30,
    frames: 100,
    player: { id: "practice-player", name: "Player", x: 0, z: 0, health: 100, maxHealth: 100 },
    target: {
      id: "practice-opponent",
      name: "Enemy Bot",
      x: 100,
      z: 100,
      health: 75,
      maxHealth: 100,
    },
    selected: true,
    distance: 1,
    range: 6,
    cooldown: 0,
    attackStatus: "Ready",
    feedback: "Hit",
    won: false,
    character: { status: "placeholder" },
    camera: { yaw: 0, pitch: 1, distance: 5 },
    pulse: { damage: 25, cooldown: 1, range: 6 },
    ...overrides,
  };
}
