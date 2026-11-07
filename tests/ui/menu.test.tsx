import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it } from "vitest";
import { App } from "../../web/src/app/App";
import { initial } from "../../web/src/domain/preferences/schema";
import { engine, error, focus, interaction, selectedLayout } from "../../web/src/state/session";
import { preferences, persistence, saveMessage } from "../../web/src/state/config";
import { pendingBinding } from "../../web/src/state/binding-capture";
import { installInput } from "../../web/src/runtime/input-router";
import { endDrag } from "../../web/src/runtime/spell-drag";
import { snapshot } from "./fixtures";

let router: ReturnType<typeof installInput>;

beforeEach(() => {
  document.body.innerHTML = '<canvas id="arena" tabindex="0"></canvas>';
  preferences.set(initial());
  engine.set(snapshot());
  error.set("");
  focus.set(null);
  saveMessage.set("");
  pendingBinding.set(null);
  selectedLayout.set(null);
  interaction.set({ panel: null, capture: null, quick: false, layout: false, hovered: null });
  endDrag();
  const canvas = document.getElementById("arena") as HTMLCanvasElement;
  router = installInput(canvas, () => {});
  canvas.focus();
});

afterEach(() => {
  router.dispose();
  cleanup();
  clearTimeout(persistence.timer);
});

it("has no top-right toolbar or unlocked warning and opens the menu with Escape", async () => {
  const user = userEvent.setup();
  render(<App />);
  expect(document.querySelector(".topbar")).toBeNull();
  expect(document.querySelector(".assignment-notice")).toBeNull();
  expect(screen.queryByRole("button", { name: "Settings" })).toBeNull();
  await user.keyboard("{Escape}");
  expect(screen.getByRole("dialog", { name: "Game menu" })).toBeTruthy();
  expect(screen.getByRole("dialog").classList.contains("window-area-left")).toBe(false);
  expect(router.sample()[4]).toBe(0);
  await user.click(screen.getByRole("button", { name: "Resume" }));
  await waitFor(() => expect(document.activeElement?.id).toBe("arena"));
  await user.keyboard("w");
  expect(interaction.get().panel).toBeNull();
});

it("reaches settings from the menu and exposes a configurable menu action", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.keyboard("{Escape}");
  await user.click(screen.getByRole("button", { name: "Settings" }));
  expect(screen.getByRole("dialog", { name: "Settings" })).toBeTruthy();
  expect(screen.getByRole("dialog").classList.contains("window-area-left")).toBe(false);
  await user.type(screen.getByLabelText("Search actions"), "menu");
  expect(screen.getByRole("button", { name: "Bind menu primary" })).toBeTruthy();
  await user.click(screen.getByRole("button", { name: "Bind menu primary" }));
  await user.keyboard("{Escape}");
  expect(interaction.get().capture).toBeNull();
  expect(preferences.get().profiles[0].config.bindings.menu[0]?.code).toBe("Escape");
});

it("reaches the movable direct-selection layout editor from the menu", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.keyboard("{Escape}");
  await user.click(screen.getByRole("button", { name: "Edit layout" }));
  expect(screen.getByRole("complementary", { name: "Layout editor" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "Move Layout editor" })).toBeTruthy();
  await user.click(screen.getByRole("button", { name: "Edit focus" }));
  expect(screen.getByText("Editing focus")).toBeTruthy();
  expect(screen.queryByRole("combobox")).toBeNull();
  expect(screen.getAllByRole("meter")).toHaveLength(8);
  act(() => {
    interaction.set({ ...interaction.get(), layout: false });
  });
});

it("opens Spells from the menu without taking gameplay focus", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.keyboard("{Escape}");
  await user.click(screen.getByRole("button", { name: "Spells" }));
  await waitFor(() => expect(document.activeElement?.id).toBe("arena"));
  expect(screen.getByRole("dialog", { name: "Spells" })).toBeTruthy();
  expect(screen.getByRole("dialog").classList.contains("window-area-left")).toBe(true);
  expect(screen.queryByRole("dialog", { name: "Spellbook" })).toBeNull();
  const spells = screen.getByRole("group", { name: "Known spells" });
  expect(spells.querySelectorAll("button")).toHaveLength(1);
  expect(spells.querySelector("small")).toBeNull();
  fireEvent.keyDown(document.activeElement!, { code: "KeyW", key: "w" });
  expect(router.sample()[1]).toBe(-1);
  expect(
    screen.getByRole("button", { name: "Pulse spell" }).getAttribute("data-spell-draggable"),
  ).toBe("true");
  expect(screen.queryByRole("combobox")).toBeNull();
});

it("opens Spells with the existing P binding and names it in binding settings", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.keyboard("p");
  expect(screen.getByRole("dialog", { name: "Spells" })).toBeTruthy();
  await user.click(screen.getByRole("button", { name: "Close Spells" }));
  await user.keyboard("o");
  await user.type(screen.getByLabelText("Search actions"), "spells");
  expect(screen.getByRole("button", { name: "Bind Spells primary" })).toBeTruthy();
  expect(preferences.get().profiles[0].config.bindings.spellbook[0]?.code).toBe("KeyP");
});
