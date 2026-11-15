import type { UnitTag } from "../domain/units/tags";
import { createStore } from "../state/store";

export const hoveredFrame = createStore<UnitTag | null>(null);

// Canvas-local CSS coordinates, independent of click/selection intent.
export function installUnitHover(canvas: HTMLCanvasElement) {
  const controller = new AbortController();
  const options = { signal: controller.signal };
  let cursor: number[] = [];
  const clear = () => {
    cursor = [];
    hoveredFrame.set(null);
  };
  document.addEventListener(
    "mousemove",
    (e) => {
      cursor = [];
      if (e.target !== canvas || e.buttons || document.pointerLockElement) return;
      const rect = canvas.getBoundingClientRect();
      cursor = [e.clientX - rect.left, e.clientY - rect.top];
    },
    options,
  );
  canvas.addEventListener(
    "mouseleave",
    () => {
      cursor = [];
    },
    options,
  );
  document.addEventListener("mousedown", clear, options);
  document.addEventListener("pointerlockchange", clear, options);
  document.addEventListener("visibilitychange", clear, options);
  document.addEventListener(
    "keydown",
    (e) => {
      if (e.key === "Escape") clear();
    },
    options,
  );
  window.addEventListener("blur", clear, options);
  return {
    read: () => cursor,
    dispose() {
      controller.abort();
      clear();
    },
  };
}
