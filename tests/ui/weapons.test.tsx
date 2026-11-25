import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  defaultWeapons,
  parseWeapons,
  weaponColors,
  weaponKinds,
} from "../../web/src/domain/weapons";
import { Settings } from "../../web/src/features/settings/Settings";
import { activateSlot, dispatch, input } from "../../web/src/runtime/commands";
import { engine, error, interaction } from "../../web/src/state/session";
import {
  WEAPONS_KEY,
  loadWeapons,
  readWeapons,
  sheathed,
  toggleSheath,
  updateWeapons,
  weapons,
  weaponsSaveError,
} from "../../web/src/state/weapons";
import { snapshot } from "./fixtures";

beforeEach(() => {
  localStorage.removeItem(WEAPONS_KEY);
  weapons.set({ ...defaultWeapons });
  weaponsSaveError.set("");
  sheathed.set(true);
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

it("switches both weapons through every palette and persists the presentation callback", () => {
  render(<Settings />);
  fireEvent.click(screen.getByRole("button", { name: "Weapons" }));
  for (const [kindIndex, weapon] of weaponKinds.entries()) {
    fireEvent.change(screen.getByLabelText("Weapon"), { target: { value: weapon } });
    for (const [colorIndex, color] of weaponColors.entries()) {
      fireEvent.change(screen.getByLabelText("Energy color"), { target: { value: color } });
      expect(readWeapons()).toEqual([kindIndex, colorIndex, 1]);
      expect(loadWeapons(localStorage)).toEqual({ version: 1, weapon, color });
    }
  }
  expect(screen.getByText(/Combat stats and abilities stay unchanged/)).toBeTruthy();
});

it("preserves malformed/future storage on load and rejects invalid runtime updates", () => {
  for (const raw of ["{", "null", '{"version":2}', '{"version":1,"weapon":"axe","color":"red"}']) {
    localStorage.setItem(WEAPONS_KEY, raw);
    expect(loadWeapons(localStorage)).toEqual(defaultWeapons);
    expect(localStorage.getItem(WEAPONS_KEY)).toBe(raw);
  }
  expect(parseWeapons({ ...defaultWeapons, color: 3 })).toBeNull();
  updateWeapons({ weapon: "invalid" as "staff" });
  expect(readWeapons()).toEqual([0, 0, 1]);
  expect(
    loadWeapons({
      getItem: () => {
        throw new Error("denied");
      },
    }),
  ).toEqual(defaultWeapons);
});

it("applies the appearance and shows a warning when persistence fails", () => {
  vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
    throw new Error("full");
  });
  render(<Settings />);
  fireEvent.click(screen.getByRole("button", { name: "Weapons" }));
  fireEvent.change(screen.getByLabelText("Weapon"), { target: { value: "staff" } });
  fireEvent.change(screen.getByLabelText("Energy color"), { target: { value: "cyan" } });
  expect(readWeapons()).toEqual([1, 5, 1]);
  expect(screen.getByRole("status").textContent).toContain("could not be saved");
});

it("the sheath keybind toggles the sword between back and hand, unsaved", () => {
  expect(readWeapons()[2]).toBe(1); // fighters start with the sword on the back
  toggleSheath();
  expect(readWeapons()[2]).toBe(0);
  toggleSheath();
  expect(readWeapons()[2]).toBe(1);
  expect(localStorage.getItem(WEAPONS_KEY)).toBeNull();
});

it("the keybind action toggles the sheath and using an ability draws the sword", () => {
  input.cancel();
  engine.set(snapshot());
  error.set("");
  interaction.set({ panel: null, quick: false, layout: false, hovered: null, capture: null });
  dispatch("toggleSheath");
  expect(sheathed.get()).toBe(false);
  dispatch("toggleSheath");
  expect(sheathed.get()).toBe(true);
  activateSlot("bar-1-s1");
  expect(sheathed.get()).toBe(false);
});
