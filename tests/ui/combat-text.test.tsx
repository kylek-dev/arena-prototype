import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { CombatText } from "../../web/src/features/combat-text/CombatText";
import { CombatErrors } from "../../web/src/features/combat-errors/CombatErrors";
import { adaptSnapshot } from "../../web/src/bridge/snapshot";
import { error, nameplateScene } from "../../web/src/state/session";
import { castError } from "../../web/src/runtime/cast-feedback";
import { nameplateNotice, setNameplates } from "../../web/src/runtime/nameplate-feedback";
import { config, persistence } from "../../web/src/state/config";
import { snapshot } from "./fixtures";

afterEach(() => {
  cleanup();
  clearTimeout(persistence.timer);
  nameplateNotice.set(null);
  castError.set(null);
  vi.useRealTimers();
});

it("renders only own outgoing numbers and follows render-rate positions and expiry", () => {
  const entry = {
    id: 1,
    source: "Player",
    target: "Opponent1",
    delta: -25,
    x: 0.4,
    y: 0.3,
    progress: 0,
  };
  error.set("");
  nameplateScene.set(
    snapshot({
      combatText: [
        entry,
        { ...entry, id: 2, delta: 20 },
        { ...entry, id: 3, source: "Opponent1" },
        { ...entry, id: 4, target: "Player" },
      ],
    }),
  );
  const { container } = render(<CombatText />);
  expect(container.querySelectorAll("span")).toHaveLength(2);
  expect(screen.getByText("25").className).toBe("damage");
  expect(screen.getByText("+20").className).toBe("healing");
  act(() => nameplateScene.set(snapshot({ combatText: [{ ...entry, x: 0.6, progress: 0.9 }] })));
  expect(screen.getByText("25").style.left).toBe("60%");
  expect(Number(screen.getByText("25").style.opacity)).toBeLessThan(0.31);
  act(() => nameplateScene.set(snapshot({ combatText: [] })));
  expect(container.querySelectorAll("span")).toHaveLength(0);
  act(() => nameplateScene.set(snapshot()));
  expect(container.querySelectorAll("span")).toHaveLength(0);
});

it("rejects invalid or duplicated combat text projections", () => {
  const entry = {
    id: 1,
    source: "Player",
    target: "Opponent1",
    delta: -25,
    x: 0.4,
    y: 0.3,
    progress: 0,
  };
  for (const combatText of [
    [{ ...entry, x: NaN }],
    [{ ...entry, delta: 0 }],
    [entry, entry],
    [{ ...entry, progress: 2 }],
  ]) {
    expect(() => adaptSnapshot({ ...snapshot(), maxHealth: 100, combatText })).toThrow(
      /combat text/,
    );
  }
});

it("announces friendly/enemy toggles in yellow alongside errors, expires and restarts", () => {
  vi.useFakeTimers();
  nameplateNotice.set(null);
  castError.set({ message: "Out of range" });
  const { container } = render(<CombatErrors />);
  act(() => setNameplates("enemy", !config().nameplates.enemy));
  expect(container.querySelector(".nameplate-notice")?.textContent).toMatch(
    /Enemy nameplates (enabled|disabled)/,
  );
  expect(screen.getByText("Out of range")).toBeTruthy();
  act(() => vi.advanceTimersByTime(2000));
  act(() => setNameplates("friendly", !config().nameplates.friendly));
  act(() => vi.advanceTimersByTime(1000));
  expect(container.querySelector(".nameplate-notice")?.textContent).toMatch(/Friendly nameplates/);
  act(() => vi.advanceTimersByTime(1500));
  expect(container.querySelector(".nameplate-notice")).toBeNull();
});
