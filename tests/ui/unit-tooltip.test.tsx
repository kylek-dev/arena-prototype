import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it } from "vitest";
import { Frames } from "../../web/src/features/unit-frames/Frames";
import { UnitTooltip } from "../../web/src/features/unit-frames/UnitTooltip";
import { LayoutSettings } from "../../web/src/features/layout-editor/LayoutSettings";
import { defaultConfig, validateConfig } from "../../web/src/domain/preferences/schema";
import { hoveredFrame, installUnitHover } from "../../web/src/runtime/unit-hover";
import { input } from "../../web/src/runtime/commands";
import { updateConfig, config } from "../../web/src/state/config";
import { draggingSpell, engine, error, focus, interaction } from "../../web/src/state/session";
import { snapshot } from "./fixtures";

beforeEach(() => {
  input.cancel();
  hoveredFrame.set(null);
  engine.set(snapshot());
  focus.set(null);
  error.set("");
  draggingSpell.set(false);
  interaction.set({ panel: null, quick: false, layout: false, hovered: null, capture: null });
  updateConfig(() => defaultConfig());
});
afterEach(cleanup);

it("shows one shared tooltip for all frame tags without selecting or focusing", () => {
  focus.set(snapshot().target.id);
  render(
    <>
      <Frames />
      <UnitTooltip />
    </>,
  );
  for (const role of ["player", "target", "focus", "opponent"]) {
    const frame = screen.getByRole("region", { name: `${role} status` }).closest("button")!;
    fireEvent.mouseEnter(frame);
    const tooltip = screen.getByRole("tooltip");
    expect(within(tooltip).getByText(role === "player" ? "Player" : "Enemy Bot")).toBeTruthy();
    expect(
      within(tooltip).getByText("Resource placeholder · no resource costs or regeneration yet"),
    ).toBeTruthy();
    fireEvent.mouseLeave(frame);
    expect(screen.queryByRole("tooltip")).toBeNull();
  }
  expect(input.select).toBe(false);
  expect(input.selectPlayer).toBe(false);
  expect(focus.get()).toBe(snapshot().target.id);
});

it("uses live world identity/health, clears misses, and supports older snapshots", () => {
  render(<UnitTooltip />);
  expect(screen.queryByRole("tooltip")).toBeNull();
  act(() => engine.set(snapshot({ hoveredUnit: "Player" })));
  expect(within(screen.getByRole("tooltip")).getByText("Player")).toBeTruthy();
  act(() =>
    engine.set(snapshot({ hoveredUnit: "Opponent1", target: { ...snapshot().target, health: 0 } })),
  );
  expect(screen.getByText("0/100")).toBeTruthy();
  expect(screen.getByText("Defeated")).toBeTruthy();
  act(() => engine.set(snapshot({ hoveredUnit: null })));
  expect(screen.queryByRole("tooltip")).toBeNull();
});

it("suppresses during exclusive modes and drags but previews/selects in layout mode", () => {
  hoveredFrame.set("Player");
  render(
    <>
      <UnitTooltip />
      <LayoutSettings />
    </>,
  );
  expect(screen.getByRole("tooltip")).toBeTruthy();
  act(() => draggingSpell.set(true));
  expect(screen.queryByRole("tooltip")).toBeNull();
  act(() => draggingSpell.set(false));
  for (const change of [
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
        ...change,
      }),
    );
    expect(screen.queryByRole("tooltip")).toBeNull();
  }
  act(() =>
    interaction.set({ panel: null, quick: false, layout: true, hovered: null, capture: null }),
  );
  fireEvent.click(screen.getByRole("button", { name: "Edit unitTooltip" }));
  expect(screen.getByText("Editing unitTooltip")).toBeTruthy();
  fireEvent.click(screen.getByLabelText("Visible"));
  expect(config().frames.unitTooltip.visible).toBe(false);
  expect(screen.getByText("Unit tooltip · layout preview")).toBeTruthy();
  act(() => interaction.set({ ...interaction.get(), layout: false }));
  expect(screen.queryByRole("tooltip")).toBeNull();
});

it("adds bottom-right placement to old profiles and round trips custom layout", () => {
  const old = {
    ...defaultConfig(),
    frames: { player: { ...defaultConfig().frames.player, x: 0.4 } },
  };
  const migrated = validateConfig(old);
  expect(migrated.frames.player.x).toBe(0.4);
  expect(migrated.frames.unitTooltip).toEqual({
    x: 1,
    y: 1,
    scale: 1,
    visible: true,
    locked: false,
  });
  migrated.frames.unitTooltip = { x: 0.3, y: 0.6, scale: 1.2, visible: false, locked: true };
  expect(validateConfig(JSON.parse(JSON.stringify(migrated))).frames.unitTooltip).toEqual(
    migrated.frames.unitTooltip,
  );
});

it("only publishes unpressed canvas coordinates and clears on exit, blur, escape and disposal", () => {
  const canvas = document.createElement("canvas");
  document.body.append(canvas);
  const hover = installUnitHover(canvas);
  const move = () => fireEvent.mouseMove(canvas, { clientX: 20, clientY: 30 });
  move();
  expect(hover.read()).toEqual([20, 30]);
  fireEvent.mouseMove(document.body);
  expect(hover.read()).toEqual([]);
  move();
  fireEvent.mouseLeave(canvas);
  expect(hover.read()).toEqual([]);
  move();
  fireEvent.mouseDown(canvas);
  expect(hover.read()).toEqual([]);
  fireEvent.mouseMove(canvas, { buttons: 1 });
  expect(hover.read()).toEqual([]);
  move();
  fireEvent.blur(window);
  expect(hover.read()).toEqual([]);
  move();
  fireEvent.keyDown(document, { key: "Escape" });
  expect(hover.read()).toEqual([]);
  hover.dispose();
  move();
  expect(hover.read()).toEqual([]);
  canvas.remove();
});
