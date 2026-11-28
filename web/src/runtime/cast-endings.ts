import type { CastEnd, CastOutcome, Snapshot } from "../bridge/snapshot";
import type { SpellKey } from "../domain/casting";
import { createStore } from "../state/store";

/** How long each ending plays on a cast bar (ms); see unit-frames.css. */
export const ENDING_MS: Record<CastOutcome, number> = {
  completed: 900,
  cancelled: 1000,
  interrupted: 1200,
};

export type CastEnding = { id: number; spell: SpellKey; outcome: CastOutcome };

/** Each unit's cast ending now playing, by entity id. */
export const castEndings = createStore<ReadonlyMap<string, CastEnding>>(new Map());

let seen = new Set<number>();
const timers = new Map<string, ReturnType<typeof setTimeout>>();

/** Starts the endings new in this snapshot. The engine repeats each ending
 * for about a second, so one is new when the previous snapshot lacked its id. */
export function observeCastEnds(next: Snapshot) {
  const ends = next.castEnds ?? [];
  const fresh = ends.filter((end) => !seen.has(end.id));
  seen = new Set(ends.map((end) => end.id));
  if (fresh.length) start(fresh);
}

function start(fresh: CastEnd[]) {
  const map = new Map(castEndings.get());
  for (const end of fresh) {
    const ending = { id: end.id, spell: end.spell, outcome: end.outcome };
    map.set(end.unit, ending);
    clearTimeout(timers.get(end.unit));
    timers.set(
      end.unit,
      setTimeout(() => {
        timers.delete(end.unit);
        if (castEndings.get().get(end.unit)?.id !== ending.id) return;
        const after = new Map(castEndings.get());
        after.delete(end.unit);
        castEndings.set(after);
      }, ENDING_MS[end.outcome]),
    );
  }
  castEndings.set(map);
}

/** Forget every ending (a new match or practice round). */
export function clearCastEndings() {
  for (const timer of timers.values()) clearTimeout(timer);
  timers.clear();
  seen = new Set();
  castEndings.set(new Map());
}
