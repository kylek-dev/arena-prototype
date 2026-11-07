import { useLayoutEffect, useRef } from "react";
import { cooldownAnimator } from "../../runtime/cooldown-animation";

export function Cooldown() {
  const element = useRef<HTMLSpanElement>(null);

  useLayoutEffect(
    () =>
      cooldownAnimator.subscribe((visual) => {
        const node = element.current;
        if (!node) return;
        node.hidden = !visual.active;
        node.style.setProperty("--cooldown-sweep", `${visual.sweep}deg`);
        // Avoid replacing text every frame when the tenth-second label is unchanged.
        if (node.textContent !== visual.label) node.textContent = visual.label;
      }),
    [],
  );

  // Availability/tooltips expose authoritative text. Per-frame decorative
  // updates must not flood assistive technology or reconcile the entire bar.
  return <span ref={element} className="cooldown" aria-hidden="true" />;
}
