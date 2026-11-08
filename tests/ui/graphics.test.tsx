import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { defaultGraphics, graphicsValues, parseGraphics } from "../../web/src/domain/graphics";
import { Settings } from "../../web/src/features/settings/Settings";
import {
  GRAPHICS_KEY,
  graphics,
  graphicsSaveError,
  loadGraphics,
  readGraphics,
  updateGraphics,
} from "../../web/src/state/graphics";

beforeEach(() => {
  localStorage.removeItem(GRAPHICS_KEY);
  graphics.set({ ...defaultGraphics });
  graphicsSaveError.set("");
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

it("opens Graphics from settings and applies, persists and resets quality", () => {
  render(<Settings />);
  fireEvent.click(screen.getByRole("button", { name: "Graphics" }));
  expect(readGraphics()).toEqual([4, 1, 1]);
  fireEvent.change(screen.getByLabelText("Anti-aliasing"), { target: { value: "off" } });
  fireEvent.change(screen.getByLabelText("Model detail"), { target: { value: "low" } });
  expect(readGraphics()).toEqual([1, 0, 1]);
  expect(graphicsValues(loadGraphics(localStorage))).toEqual([1, 0, 1]);
  fireEvent.click(screen.getByRole("checkbox", { name: "Weapon glow (bloom)" }));
  expect(readGraphics()).toEqual([1, 0, 0]);
  expect(loadGraphics(localStorage).bloom).toBe(false);
  fireEvent.click(screen.getByRole("button", { name: "Reset graphics" }));
  expect(loadGraphics(localStorage)).toEqual(defaultGraphics);
  expect(readGraphics()).toEqual([4, 1, 1]);
});

it("rejects malformed and future storage without overwriting it on load", () => {
  for (const raw of ["{", "null", '{"version":2}', '{"version":1,"antialiasing":"8x"}']) {
    localStorage.setItem(GRAPHICS_KEY, raw);
    expect(loadGraphics(localStorage)).toEqual(defaultGraphics);
    expect(localStorage.getItem(GRAPHICS_KEY)).toBe(raw);
  }
  expect(parseGraphics({ ...defaultGraphics, modelDetail: null })).toBeNull();
  expect(parseGraphics({ ...defaultGraphics, antialiasing: 4 })).toBeNull();
  expect(
    loadGraphics({
      getItem: () => {
        throw new Error("denied");
      },
    }),
  ).toEqual(defaultGraphics);
});

it("still applies settings when storage fails and shows the failure", () => {
  vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
    throw new Error("full");
  });
  render(<Settings />);
  fireEvent.click(screen.getByRole("button", { name: "Graphics" }));
  fireEvent.change(screen.getByLabelText("Anti-aliasing"), { target: { value: "off" } });
  expect(readGraphics()).toEqual([1, 1, 1]);
  expect(screen.getByRole("status").textContent).toContain("could not be saved");
});

it("keeps the graphics callback separate from combat and rejects invalid updates", () => {
  updateGraphics({ modelDetail: "low" });
  expect(readGraphics()).toEqual([4, 0, 1]);
  // Runtime boundary remains validated even if callers bypass TypeScript.
  updateGraphics({ antialiasing: "invalid" as "off" });
  expect(readGraphics()).toEqual([4, 0, 1]);
  updateGraphics({ bloom: "invalid" as unknown as boolean });
  expect(readGraphics()).toEqual([4, 0, 1]);
});

it("upgrades existing v1 graphics preferences without overwriting storage on load", () => {
  const old = JSON.stringify({ version: 1, antialiasing: "off", modelDetail: "low" });
  localStorage.setItem(GRAPHICS_KEY, old);
  expect(graphicsValues(loadGraphics(localStorage))).toEqual([1, 0, 1]);
  expect(localStorage.getItem(GRAPHICS_KEY)).toBe(old);
  expect(parseGraphics({ ...defaultGraphics, bloom: null })).toBeNull();
});
