import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { CastEnd } from "../../web/src/bridge/snapshot";
import type { Entity } from "../../web/src/domain/units/selection";
import { PlayerCastBar } from "../../web/src/features/cast-bar/PlayerCastBar";
import { UnitFrame } from "../../web/src/features/unit-frames/UnitFrame";
import { LOW_HEALTH } from "../../web/src/features/unit-frames/health-effects";
import { clearCastEndings, ENDING_MS, observeCastEnds } from "../../web/src/runtime/cast-endings";
import { engine, interaction } from "../../web/src/state/session";
import { snapshot } from "./fixtures";

const unit = (more: Partial<Entity> = {}): Entity => ({
  id: "practice-opponent",
  name: "Enemy Bot",
  health: 80,
  maxHealth: 100,
  resource: 60,
  maxResource: 100,
  ...more,
});
const rift = (remainingTicks: number) =>
  ({ spell: "rift_bolt", remainingTicks, totalTicks: 45 }) as const;
const ended = (id: number, outcome: CastEnd["outcome"], who = "practice-opponent") =>
  act(() =>
    observeCastEnds(snapshot({ castEnds: [{ id, unit: who, spell: "rift_bolt", outcome }] })),
  );

beforeEach(() => {
  vi.useFakeTimers();
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => {});
  clearCastEndings();
  engine.set(snapshot({ tickRate: 30 }));
  interaction.set({ panel: null, capture: null, quick: false, layout: false, hovered: null });
});
afterEach(() => {
  cleanup();
  clearCastEndings();
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

it("shows how a cast ended on the frame's cast bar, then clears it", () => {
  const { rerender } = render(<UnitFrame role="target" unit={unit({ cast: rift(18) })} />);
  expect(screen.getByRole("progressbar", { name: "target casting Rift Bolt" })).toBeTruthy();
  rerender(<UnitFrame role="target" unit={unit({ cast: null })} />);
  ended(1, "cancelled");
  const bar = screen.getByRole("status", { name: "target cast Rift Bolt cancelled" });
  expect(bar.classList).toContain("cancelled");
  expect(bar.textContent).toBe("Cancelled");
  // It drains back from where the cast stood: 27 of 45 ticks.
  expect(bar.querySelector<HTMLElement>(".cast-fill")?.style.width).toBe("60%");
  act(() => vi.advanceTimersByTime(ENDING_MS.cancelled));
  expect(screen.queryByRole("status")).toBeNull();

  ended(2, "interrupted");
  const interrupted = screen.getByRole("status", { name: "target cast Rift Bolt interrupted" });
  expect(interrupted.textContent).toBe("Interrupted");
  expect(interrupted.querySelector<HTMLElement>(".cast-fill")?.style.width).toBe("100%");

  ended(3, "completed");
  const completed = screen.getByRole("status", { name: "target cast Rift Bolt completed" });
  expect(completed.textContent).toBe("Rift Bolt");
  expect(completed.querySelector(".cast-burst")).toBeTruthy();
});

it("prefers a new cast over the last one's ending, and ignores other units' endings", () => {
  const { rerender } = render(<UnitFrame role="target" unit={unit({ cast: null })} />);
  ended(1, "completed", "practice-actor-3");
  expect(screen.queryByRole("status")).toBeNull();
  ended(2, "completed");
  expect(screen.getByRole("status")).toBeTruthy();
  rerender(<UnitFrame role="target" unit={unit({ cast: rift(40) })} />);
  expect(screen.queryByRole("status")).toBeNull();
  expect(screen.getByRole("progressbar", { name: "target casting Rift Bolt" })).toBeTruthy();
});

it("plays the player's ending on the separate cast bar when it is not attached", () => {
  engine.set(snapshot({ player: { ...snapshot().player, cast: rift(10) } }));
  render(<PlayerCastBar />);
  expect(screen.getByRole("progressbar", { name: "player casting Rift Bolt" })).toBeTruthy();
  act(() => engine.set(snapshot()));
  expect(document.querySelector(".player-cast-bar")).toBeNull();
  ended(1, "interrupted", "practice-player");
  const bar = screen.getByRole("status", { name: "player cast Rift Bolt interrupted" });
  expect(bar.closest(".player-cast-bar")).toBeTruthy();
  act(() => vi.advanceTimersByTime(ENDING_MS.interrupted));
  expect(document.querySelector(".player-cast-bar")).toBeNull();
});

it("leaves a draining trail for health just lost and a wave for health gained", () => {
  const { container, rerender } = render(<UnitFrame role="target" unit={unit()} />);
  const frame = container.querySelector(".unit-frame")!;
  expect(container.querySelector(".health-trail")).toBeNull();
  rerender(<UnitFrame role="target" unit={unit({ health: 50 })} />);
  const trail = container.querySelector<HTMLElement>(".health-trail")!;
  expect([trail.style.left, trail.style.width]).toEqual(["50%", "30%"]);
  // A second hit while it drains extends it from the first loss.
  rerender(<UnitFrame role="target" unit={unit({ health: 40 })} />);
  const again = container.querySelector<HTMLElement>(".health-trail")!;
  expect([again.style.left, again.style.width]).toEqual(["40%", "40%"]);

  rerender(<UnitFrame role="target" unit={unit({ health: 70 })} />);
  expect(container.querySelector(".health-trail")).toBeNull();
  expect(frame.classList).toContain("healing");
  expect(container.querySelector(".health-heal-wave")).toBeTruthy();
  act(() => vi.advanceTimersByTime(700));
  expect(frame.classList).not.toContain("healing");

  // A different unit in the frame (a new target) starts clean.
  rerender(<UnitFrame role="target" unit={unit({ id: "practice-actor-3", health: 20 })} />);
  expect(container.querySelector(".health-trail")).toBeNull();
  expect(frame.classList).not.toContain("healing");
});

it("pulses at low health for living, connected units only", () => {
  const low = (more: Partial<Entity>) => {
    const { container, unmount } = render(<UnitFrame role="opponent" unit={unit(more)} />);
    const result = container.querySelector(".unit-frame")!.classList.contains("low-health");
    unmount();
    return result;
  };
  expect(low({ health: LOW_HEALTH * 100 })).toBe(true);
  expect(low({ health: LOW_HEALTH * 100 + 1 })).toBe(false);
  expect(low({ health: 0 })).toBe(false);
  expect(low({ health: 10, disconnected: true })).toBe(false);
});

it("puts the spark on partly filled bars only", () => {
  const sparks = (more: Partial<Entity>) => {
    const { container, unmount } = render(<UnitFrame role="target" unit={unit(more)} />);
    const result = [".health-fill", ".resource-fill"].map(
      (fill) => !!container.querySelector(`${fill} .bar-spark`),
    );
    unmount();
    return result;
  };
  expect(sparks({})).toEqual([true, true]);
  expect(sparks({ health: 100, resource: 0 })).toEqual([false, false]);
});

it("mirrors the frame art for the units you face and shows status in its socket", () => {
  const mirrored = (role: Parameters<typeof UnitFrame>[0]["role"]) => {
    const { container, unmount } = render(<UnitFrame role={role} unit={unit()} />);
    const result = container.querySelector(".unit-frame")!.classList.contains("mirrored");
    unmount();
    return result;
  };
  expect(
    ["player", "teammate", "target", "focus", "opponent"].map((r) => mirrored(r as never)),
  ).toEqual([false, false, true, true, true]);
  const { container } = render(<UnitFrame role="target" unit={unit({ lockoutRemaining: 20 })} />);
  expect(container.querySelector(".unit-socket")?.textContent).toBe("Locked out");
});

it("draws the frame without its portrait when hidden, the status in the name plaque", () => {
  const { container } = render(
    <UnitFrame role="target" unit={unit({ lockoutRemaining: 20 })} portrait={false} />,
  );
  const frame = container.querySelector(".unit-frame")!;
  expect(frame.classList).toContain("bare");
  expect(frame.querySelector(".unit-socket")).toBeNull();
  expect(frame.querySelector(".unit-heading .unit-status")?.textContent).toBe("Locked out");
});
