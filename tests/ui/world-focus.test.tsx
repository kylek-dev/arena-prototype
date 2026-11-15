import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { initial } from "../../web/src/domain/preferences/schema";
import { preferences } from "../../web/src/state/config";
import { draggingSpell, engine, error, focus, interaction } from "../../web/src/state/session";
import { input } from "../../web/src/runtime/commands";
import { startRuntime } from "../../web/src/runtime/start";
import { snapshot } from "./fixtures";

vi.mock("../../web/src/bridge/loader", () => ({
  loadEngine: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("../../web/src/runtime/diagnostics", async (original) => ({
  ...(await original<typeof import("../../web/src/runtime/diagnostics")>()),
  measureFrames: vi.fn(() => () => {}),
}));

let stop: () => void;
beforeEach(() => {
  document.body.innerHTML = '<canvas tabindex="0"></canvas>';
  preferences.set(initial());
  engine.set(snapshot());
  focus.set(null);
  error.set("");
  draggingSpell.set(false);
  interaction.set({ panel: null, quick: false, layout: false, hovered: null, capture: null });
  input.cancel();
  stop = startRuntime(document.querySelector("canvas")!);
});
afterEach(() => stop());

it("routes picked world identities to independent player/opponent focus without target or attack intent", () => {
  for (const [tag, id] of [
    ["Player", snapshot().player.id],
    ["Opponent1", snapshot().target.id],
  ] as const) {
    window.arenaShell.onFocusUnit?.(tag);
    expect(focus.get()).toBe(id);
    expect(engine.get()?.selected).toBe(true);
    expect(input.readInput().slice(2, 6)).toEqual([0, 0, 0, 0]);
    expect(input.selectPlayer).toBe(false);
  }
});

it("guards delayed world focus while gameplay is suspended or unavailable", () => {
  for (const change of [
    { layout: true },
    { quick: true },
    { panel: "menu" as const },
    { panel: "settings" as const },
    { capture: { action: "menu", entry: 0 as const } },
  ]) {
    interaction.set({
      panel: null,
      quick: false,
      layout: false,
      hovered: null,
      capture: null,
      ...change,
    });
    window.arenaShell.onFocusUnit?.("Opponent1");
    expect(focus.get()).toBeNull();
  }
  interaction.set({ ...interaction.get(), capture: null });
  draggingSpell.set(true);
  window.arenaShell.onFocusUnit?.("Opponent1");
  expect(focus.get()).toBeNull();
  draggingSpell.set(false);
  engine.set(snapshot({ character: { status: "loading" } }));
  window.arenaShell.onFocusUnit?.("Opponent1");
  expect(focus.get()).toBeNull();
});
