import type { Snapshot } from "../bridge/snapshot";
import { CooldownTimeline, type CooldownVisual } from "../domain/abilities/cooldown-presentation";
import { engine } from "../state/session";

type Source = { get: () => Snapshot | null; subscribe: (listener: () => void) => () => void };
type FrameClock = {
  now: () => number;
  request: (callback: FrameRequestCallback) => number;
  cancel: (id: number) => void;
};

// One frame loop/timeline for all copies of Pulse, not one React render/loop per
// slot. The animator writes only presentation; activation still reads engine.
export function createCooldownAnimator(source: Source, clock: FrameClock) {
  const listeners = new Set<(visual: CooldownVisual) => void>();
  const timeline = new CooldownTimeline();
  let frame: number | null = null;
  let stopSource: (() => void) | null = null;

  function paint(now: number) {
    const visual = timeline.visual(now);
    for (const listener of listeners) listener(visual);
    return visual.active;
  }

  function schedule() {
    if (frame !== null || !listeners.size || !timeline.visual(clock.now()).active) return;
    frame = clock.request((now) => {
      frame = null;
      paint(now);
      schedule();
    });
  }

  function observe() {
    const state = source.get();
    const now = clock.now();
    timeline.observe(
      state
        ? {
            tick: state.tick,
            remaining: state.cooldown,
            duration: state.pulse.cooldown,
          }
        : null,
      now,
    );
    if (!paint(now) && frame !== null) {
      clock.cancel(frame);
      frame = null;
    }
    schedule();
  }

  return {
    subscribe(listener: (visual: CooldownVisual) => void) {
      listeners.add(listener);
      if (listeners.size === 1) {
        timeline.observe(null, clock.now());
        stopSource = source.subscribe(observe);
        observe();
      } else {
        listener(timeline.visual(clock.now()));
      }
      return () => {
        listeners.delete(listener);
        if (listeners.size) return;
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
