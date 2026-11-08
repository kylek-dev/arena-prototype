import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it } from "vitest";
import { Nameplates } from "../../web/src/features/nameplates/Nameplates";
import { NameplateSettings } from "../../web/src/features/settings/NameplateSettings";
import { config, preferences, persistence } from "../../web/src/state/config";
import { initial } from "../../web/src/domain/preferences/schema";
import {
  draggingSpell,
  engine,
  error,
  focus,
  interaction,
  nameplateScene,
} from "../../web/src/state/session";
import { input } from "../../web/src/runtime/commands";
import { snapshot, teamSnapshot } from "./fixtures";

beforeEach(() => {
  preferences.set(initial());
  input.cancel();
  engine.set(snapshot());
  nameplateScene.set(snapshot({ nameplates: [{ tag: "Opponent1", x: 0.4, y: 0.3 }] }));
  error.set("");
  focus.set(null);
  draggingSpell.set(false);
  interaction.set({ panel: null, quick: false, layout: false, hovered: null, capture: null });
});
afterEach(() => {
  cleanup();
  clearTimeout(persistence.timer);
});

it("marks the targeted plate at render rate, independently of focus and HUD delivery", () => {
  const plates = [
    { tag: "Team1" as const, x: 0.3, y: 0.4 },
    { tag: "Opponent2" as const, x: 0.5, y: 0.4 },
  ];
  engine.set(teamSnapshot(3, 1));
  focus.set("practice-actor-3");
  nameplateScene.set({ ...teamSnapshot(3, 2), nameplates: plates });
  render(<Nameplates />);
  expect(screen.getByRole("button", { pressed: true }).textContent).toBe("Teammate 1");
  act(() => nameplateScene.set({ ...teamSnapshot(3, 3), nameplates: plates }));
  expect(screen.getByRole("button", { pressed: true }).textContent).toBe("Enemy Bot 2");
  expect(engine.get()?.selectedActor).toBe(1);
  act(() => nameplateScene.set({ ...teamSnapshot(3, null), nameplates: plates }));
  expect(screen.queryByRole("button", { pressed: true })).toBeNull();
  act(() => nameplateScene.set({ ...teamSnapshot(3, 0), nameplates: plates }));
  expect(screen.queryByRole("button", { pressed: true })).toBeNull();
});

it("shows a small unit name with the health bar and targets/focuses without attacking or mouse look", () => {
  const { container } = render(<Nameplates />);
  const bar = screen.getByRole("button", { name: /Enemy Bot health 75\/100/ });
  expect(bar.textContent).toBe("Enemy Bot");
  expect(bar.querySelector(".nameplate-name")?.getAttribute("aria-hidden")).toBe("true");
  expect(screen.getAllByRole("button")).toHaveLength(1);
  expect(container.querySelector(".nameplate-fill")).toBeTruthy();
  fireEvent.click(bar);
  expect(input.readInput()[3]).toBe(1);
  expect(fireEvent.contextMenu(bar)).toBe(false);
  expect(focus.get()).toBe(snapshot().target.id);
  expect(input.select).toBe(false);
  expect(input.looking).toBe(false);
  expect(input.readInput()[2]).toBe(0);
});

it("labels friendly and enemy plates with their roster names, never the player", () => {
  const state = teamSnapshot(3);
  nameplateScene.set({
    ...state,
    nameplates: [
      { tag: "Team1", x: 0.3, y: 0.4 },
      { tag: "Team2", x: 0.4, y: 0.4 },
      { tag: "Opponent2", x: 0.5, y: 0.4 },
    ],
  });
  render(<Nameplates />);
  expect(screen.getByText("Teammate 1")).toBeTruthy();
  expect(screen.getByText("Teammate 2")).toBeTruthy();
  expect(screen.getByText("Enemy Bot 2")).toBeTruthy();
  expect(screen.queryByText("Player")).toBeNull();
});

it("follows render-rate projections and health independently of ordinary HUD delivery", () => {
  render(<Nameplates />);
  act(() =>
    nameplateScene.set(
      snapshot({
        target: { ...snapshot().target, health: 25 },
        nameplates: [{ tag: "Opponent1", x: 0.65, y: 0.2 }],
      }),
    ),
  );
  const bar = screen.getByRole("button", { name: /health 25\/100/ });
  expect(bar.style.left).toBe("65%");
  expect(bar.style.top).toBe("20%");
  expect((bar.firstElementChild as HTMLElement).style.width).toBe("25%");
  expect(engine.get()?.target.health).toBe(75);
  act(() => nameplateScene.set(snapshot({ nameplates: [] })));
  expect(screen.queryByRole("button")).toBeNull();
  act(() => nameplateScene.set(snapshot()));
  expect(screen.queryByRole("button")).toBeNull();
});

it("suppresses drag clicks and exclusive-mode interaction", () => {
  render(<Nameplates />);
  const bar = screen.getByRole("button");
  fireEvent.mouseDown(bar, { clientX: 100, clientY: 100 });
  fireEvent.click(bar, { clientX: 120, clientY: 100 });
  expect(input.select).toBe(false);
  for (const patch of [
    { layout: true },
    { quick: true },
    { panel: "menu" as const },
    { panel: "settings" as const },
    { capture: { action: "menu", entry: 0 as const } },
  ]) {
    act(() =>
      interaction.set({
        panel: null,
        quick: false,
        layout: false,
        hovered: null,
        capture: null,
        ...patch,
      }),
    );
    expect(screen.queryByRole("button")).toBeNull();
  }
  act(() => {
    interaction.set({ ...interaction.get(), capture: null });
    draggingSpell.set(true);
  });
  expect(screen.queryByRole("button")).toBeNull();
});

it("settings independently toggle each category using the saved profile configuration", () => {
  render(
    <>
      <Nameplates />
      <NameplateSettings />
    </>,
  );
  fireEvent.click(screen.getByRole("checkbox", { name: "Friendly nameplates" }));
  expect(config().nameplates).toEqual({ friendly: false, enemy: true });
  expect(screen.queryByRole("button", { name: /Enemy Bot/ })).toBeTruthy();
  fireEvent.click(screen.getByRole("checkbox", { name: "Enemy nameplates" }));
  expect(config().nameplates).toEqual({ friendly: false, enemy: false });
  expect(screen.queryByRole("button", { name: /Enemy Bot/ })).toBeNull();
});
