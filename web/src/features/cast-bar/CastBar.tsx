import { useLayoutEffect, useRef } from "react";
import { CastTimeline } from "../../domain/cast-bar";
import { castProgress, spellLabel, type ActiveCast } from "../../domain/casting";
import { castBarAnimator } from "../../runtime/cast-bar-animation";
import { engine } from "../../state/session";
import { useStore } from "../../state/store";

/**
 * A cast bar that fills smoothly every frame from the cast's progress and
 * duration, instead of jumping with each throttled HUD snapshot. It mounts
 * when a cast starts and unmounts when it ends or is interrupted, so both
 * snap immediately. `frame` is the unit-frame bar; `nameplate` the compact
 * world bar.
 */
export function CastBar(
  props:
    | { cast: ActiveCast; variant: "nameplate" }
    /** `label` is the frame progress bar's accessible name. */
    | { cast: ActiveCast; variant: "frame"; label: string },
) {
  const { cast } = props;
  const tickRate = useStore(engine, (s) => s?.tickRate ?? 30);
  const timeline = useRef<CastTimeline | null>(null);
  timeline.current ??= new CastTimeline();
  const fill = useRef<HTMLDivElement & HTMLSpanElement>(null);
  const time = useRef<HTMLSpanElement>(null);
  const paint = useRef((now: number) => {
    const visual = timeline.current!.visual(now);
    if (fill.current) fill.current.style.width = `${visual.progress * 100}%`;
    const text = `${visual.remaining.toFixed(1)}s`;
    if (time.current && time.current.textContent !== text) time.current.textContent = text;
    return visual.active && !visual.settled;
  });

  const { spell, remainingTicks, totalTicks } = cast;
  useLayoutEffect(() => {
    timeline.current!.observe(
      { cast: { spell, remainingTicks, totalTicks }, tickRate },
      castBarAnimator.now(),
    );
    paint.current(castBarAnimator.now());
    castBarAnimator.wake();
  }, [spell, remainingTicks, totalTicks, tickRate]);
  useLayoutEffect(() => castBarAnimator.subscribe((now) => paint.current(now)), []);

  if (props.variant === "nameplate") {
    return (
      <span className="nameplate-cast" aria-hidden="true">
        <span ref={fill} className="nameplate-cast-fill">
          <span className="bar-spark" />
        </span>
        <span className="nameplate-cast-label">{spellLabel(spell)}</span>
      </span>
    );
  }
  return (
    <div
      className="cast-track"
      role="progressbar"
      aria-label={props.label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(castProgress(cast) * 100)}
    >
      <div ref={fill} className="cast-fill">
        <span className="bar-spark" aria-hidden="true" />
      </div>
      <span className="cast-label">
        <span>{spellLabel(spell)}</span>
        <span ref={time} />
      </span>
    </div>
  );
}
