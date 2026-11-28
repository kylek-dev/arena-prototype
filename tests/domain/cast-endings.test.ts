import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { CastEnd } from "../../web/src/bridge/snapshot";
import {
  castEndings,
  clearCastEndings,
  ENDING_MS,
  observeCastEnds,
} from "../../web/src/runtime/cast-endings";
import { snapshot } from "../ui/fixtures";

const end = (id: number, unit: string, outcome: CastEnd["outcome"]): CastEnd => ({
  id,
  unit,
  spell: "rift_bolt",
  outcome,
});

beforeEach(() => {
  vi.useFakeTimers();
  clearCastEndings();
});
afterEach(() => {
  clearCastEndings();
  vi.useRealTimers();
});

it("plays each ending once, for as long as its animation", () => {
  observeCastEnds(snapshot({ castEnds: [end(1, "a", "completed")] }));
  expect(castEndings.get().get("a")).toEqual({ id: 1, spell: "rift_bolt", outcome: "completed" });
  // The engine repeats the ending for about a second: not a new one.
  vi.advanceTimersByTime(ENDING_MS.completed - 1);
  observeCastEnds(snapshot({ castEnds: [end(1, "a", "completed")] }));
  vi.advanceTimersByTime(1);
  expect(castEndings.get().has("a")).toBe(false);
  observeCastEnds(snapshot({ castEnds: [end(1, "a", "completed")] }));
  expect(castEndings.get().has("a")).toBe(false);
});

it("keeps one ending per unit, the newest, and times each out on its own", () => {
  observeCastEnds(snapshot({ castEnds: [end(1, "a", "cancelled"), end(2, "b", "interrupted")] }));
  vi.advanceTimersByTime(500);
  observeCastEnds(
    snapshot({
      castEnds: [end(1, "a", "cancelled"), end(2, "b", "interrupted"), end(3, "a", "completed")],
    }),
  );
  expect(castEndings.get().get("a")?.id).toBe(3);
  // The first ending's timer no longer removes the newer one.
  vi.advanceTimersByTime(ENDING_MS.cancelled - 500);
  expect(castEndings.get().get("a")?.id).toBe(3);
  vi.advanceTimersByTime(ENDING_MS.interrupted - ENDING_MS.cancelled);
  expect(castEndings.get().has("b")).toBe(false);
  vi.advanceTimersByTime(ENDING_MS.completed);
  expect(castEndings.get().size).toBe(0);
});

it("leaves the store alone when nothing new ended", () => {
  const before = castEndings.get();
  observeCastEnds(snapshot());
  observeCastEnds(snapshot({ castEnds: [] }));
  expect(castEndings.get()).toBe(before);
});
