type FrameClock = {
  now: () => number;
  request: (callback: FrameRequestCallback) => number;
  cancel: (id: number) => void;
};

/** Paints one bar for `now`; returns whether it is still animating. */
export type CastPainter = (now: number) => boolean;

// One shared frame loop for every visible cast bar (unit frames, nameplates,
// spectator frames). Bars write presentation only; React renders stay on the
// throttled snapshot cadence.
export function createCastBarAnimator(clock: FrameClock) {
  const painters = new Set<CastPainter>();
  let frame: number | null = null;

  function run(now: number) {
    frame = null;
    let active = false;
    for (const paint of painters) if (paint(now)) active = true;
    if (active) schedule();
  }

  function schedule() {
    if (frame === null && painters.size) frame = clock.request(run);
  }

  return {
    subscribe(paint: CastPainter) {
      painters.add(paint);
      if (paint(clock.now())) schedule();
      return () => {
        painters.delete(paint);
        if (!painters.size && frame !== null) {
          clock.cancel(frame);
          frame = null;
        }
      };
    },
    /** Resumes the loop after a bar observed a new sample. */
    wake() {
      schedule();
    },
    now: () => clock.now(),
  };
}

export const castBarAnimator = createCastBarAnimator({
  now: () => performance.now(),
  request: (callback) => requestAnimationFrame(callback),
  cancel: (id) => cancelAnimationFrame(id),
});
