import type { Snapshot } from "../bridge/snapshot";
import { spellCooldown, spellMaxCooldown } from "../domain/abilities/catalog";
import { CooldownTimeline, type CooldownVisual } from "../domain/abilities/cooldown-presentation";
import { ABILITY_IDS, type AbilityId } from "../domain/action-bars/model";
import { engine } from "../state/session";

type Source = { get: () => Snapshot | null; subscribe: (listener: () => void) => () => void };
type FrameClock = {
  now: () => number;
  request: (callback: FrameRequestCallback) => number;
  cancel: (id: number) => void;
};

// One frame loop with a shared timeline per ability, not one React render/loop
// per slot. The animator writes only presentation; activation still reads engine.
export function createCooldownAnimator(source: Source, clock: FrameClock) {
  const listeners = new Map<AbilityId, Set<(visual: CooldownVisual) => void>>(
    ABILITY_IDS.map((ability) => [ability, new Set()]),
  );
  const timelines = new Map<AbilityId, CooldownTimeline>(
    ABILITY_IDS.map((ability) => [ability, new CooldownTimeline()]),
  );
  let frame: number | null = null;
  let stopSource: (() => void) | null = null;

  function totalListeners() {
    let count = 0;
    for (const set of listeners.values()) count += set.size;
    return count;
  }

  function anyActive(now: number) {
    for (const ability of ABILITY_IDS) {
      if (listeners.get(ability)!.size && timelines.get(ability)!.visual(now).active) return true;
    }
    return false;
  }

  function paint(now: number) {
    let active = false;
    for (const ability of ABILITY_IDS) {
      const set = listeners.get(ability)!;
      if (!set.size) continue;
      const visual = timelines.get(ability)!.visual(now);
      if (visual.active) active = true;
      for (const listener of set) listener(visual);
    }
    return active;
  }

  function schedule() {
    if (frame !== null || !totalListeners() || !anyActive(clock.now())) return;
    frame = clock.request((now) => {
      frame = null;
      paint(now);
      schedule();
    });
  }

  function observe() {
    const state = source.get();
    const now = clock.now();
    for (const ability of ABILITY_IDS) {
      timelines.get(ability)!.observe(
        state
          ? {
              tick: state.tick,
              remaining: spellCooldown(state, ability),
              duration: spellMaxCooldown(state, ability),
            }
          : null,
        now,
      );
    }
    if (!paint(now) && frame !== null) {
      clock.cancel(frame);
      frame = null;
    }
    schedule();
  }

  return {
    subscribe(listener: (visual: CooldownVisual) => void, ability: AbilityId = "pulse") {
      const wasEmpty = totalListeners() === 0;
      const set = listeners.get(ability)!;
      set.add(listener);
      if (wasEmpty) {
        for (const timeline of timelines.values()) timeline.observe(null, clock.now());
        stopSource = source.subscribe(observe);
        observe();
      } else {
        listener(timelines.get(ability)!.visual(clock.now()));
        schedule();
      }
      return () => {
        set.delete(listener);
        if (totalListeners()) return;
        stopSource?.();
        stopSource = null;
        if (frame !== null) clock.cancel(frame);
        frame = null;
      };
    },
  };
}

export const cooldownAnimator = createCooldownAnimator(engine, {
  now: () => performance.now(),
  request: (callback) => requestAnimationFrame(callback),
  cancel: (id) => cancelAnimationFrame(id),
});
