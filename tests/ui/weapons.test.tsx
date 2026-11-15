import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  defaultWeapons,
  parseWeapons,
  weaponColors,
  weaponKinds,
} from "../../web/src/domain/weapons";
import { Settings } from "../../web/src/features/settings/Settings";
import {
  WEAPONS_KEY,
  loadWeapons,
  readWeapons,
  updateWeapons,
  weapons,
  weaponsSaveError,
} from "../../web/src/state/weapons";

beforeEach(() => {
  localStorage.removeItem(WEAPONS_KEY);
  weapons.set({ ...defaultWeapons });
  weaponsSaveError.set("");
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
      expect(readWeapons()).toEqual([kindIndex, colorIndex]);
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
  expect(readWeapons()).toEqual([0, 0]);
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
  expect(readWeapons()).toEqual([1, 5]);
  expect(screen.getByRole("status").textContent).toContain("could not be saved");
});
