import type { Snapshot } from "../../web/src/bridge/snapshot";

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
