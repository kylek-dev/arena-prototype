import { useEffect, useRef, useState } from "react";
import { move, position, type Layout } from "../domain/layout/model";

// Shared viewport-bounded positioning for HUD frames and floating editor windows.
// Drag drafts stay local; only a completed gesture commits to preferences.
export function useMovable(value: Layout, onChange: (next: Layout) => void, enabled: boolean) {
  const ref = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ width: 260, height: 110 });
  const [viewport, setViewport] = useState({ width: innerWidth, height: innerHeight });
  const [draft, setDraft] = useState<Layout | null>(null);
  const draftRef = useRef<Layout | null>(null);
  const drag = useRef<{ x: number; y: number; layout: Layout } | null>(null);
  const current = draft ?? value;
  const usable = {
    width: Math.max(1, viewport.width - 24),
    height: Math.max(1, viewport.height - 24),
  };
  const scale = Math.max(
    0.1,
    Math.min(current.scale, usable.width / size.width, usable.height / size.height),
  );
  const coordinates = position({ ...current, scale }, usable, size);
  const travel = {
    width: usable.width - size.width * scale,
    height: usable.height - size.height * scale,
  };

  useEffect(() => {
    const resize = () => setViewport({ width: innerWidth, height: innerHeight });
    addEventListener("resize", resize);
    const element = ref.current;
    const measure = () => {
      if (element)
        setSize({
          width: Math.max(1, element.offsetWidth),
          height: Math.max(1, element.offsetHeight),
        });
    };
    const observer = new ResizeObserver(measure);
    if (element) {
      observer.observe(element);
      measure();
    }
    return () => {
      removeEventListener("resize", resize);
      observer.disconnect();
    };
  }, [value.visible, enabled]);

  function cancel() {
    drag.current = null;
    draftRef.current = null;
    setDraft(null);
  }

  return {
    ref,
    style: { left: coordinates.left + 12, top: coordinates.top + 12, transform: `scale(${scale})` },
    dragProps: {
      onPointerDown(e: React.PointerEvent<HTMLElement>) {
        if (!enabled || e.button !== 0) return;
        e.preventDefault();
        e.currentTarget.setPointerCapture(e.pointerId);
        drag.current = { x: e.clientX, y: e.clientY, layout: value };
      },
      onPointerMove(e: React.PointerEvent<HTMLElement>) {
        const start = drag.current;
        if (!start) return;
        const next = move(
          start.layout,
          e.clientX - start.x,
          e.clientY - start.y,
          travel.width,
          travel.height,
        );
        draftRef.current = next;
        setDraft(next);
      },
      onPointerUp() {
        if (draftRef.current) onChange(draftRef.current);
        cancel();
      },
      onPointerCancel: cancel,
      onLostPointerCapture: cancel,
      onKeyDown(e: React.KeyboardEvent<HTMLElement>) {
        if (!enabled) return;
        const deltas: Record<string, [number, number]> = {
          ArrowLeft: [-10, 0],
          ArrowRight: [10, 0],
          ArrowUp: [0, -10],
          ArrowDown: [0, 10],
        };
        const delta = deltas[e.code];
        if (!delta) return;
        e.preventDefault();
        onChange(move(value, ...delta, travel.width, travel.height));
      },
    },
  };
}
