import { input } from "./commands";
import { blocksGameplay, draggingSpell, engine, error } from "../state/session";
import { isEngineReady } from "../domain/abilities/catalog";
export function installPointer(canvas: HTMLCanvasElement, cancelKeyboard: () => void) {
  const controller = new AbortController(),
    options = { signal: controller.signal };
  let captureRequested = false;
  const end = () => {
    captureRequested = false;
    canvas.style.cursor = "";
    if (document.pointerLockElement === canvas) document.exitPointerLock();
  };
  const capture = () => {
    if (!input.looking || captureRequested) return;
    captureRequested = true;
    canvas.style.cursor = "none";
    try {
      canvas.requestPointerLock?.()?.catch(() => {});
    } catch {
      /* nonfatal in-page fallback */
    }
  };
  canvas.addEventListener("contextmenu", (e) => e.preventDefault(), options);
  canvas.addEventListener(
    "mousedown",
    (e) => {
      if (
        draggingSpell.get() ||
        error.get() ||
        !isEngineReady(engine.get()) ||
        blocksGameplay() ||
        (e.button !== 0 && e.button !== 2)
      )
        return;
      e.preventDefault();
      canvas.focus({ preventScroll: true });
      const rect = canvas.getBoundingClientRect();
      input.mouseDown({
        button: e.button,
        buttons: e.buttons,
        x: e.clientX - rect.left,
        y: e.clientY - rect.top,
      });
      capture();
    },
    options,
  );
  document.addEventListener(
    "mousemove",
    (e) => {
      input.mouseMove(e);
      capture();
    },
    options,
  );
  document.addEventListener(
    "mouseup",
    (e) => {
      if (input.mouseUp(e) && !input.dragging) end();
    },
    options,
  );
  document.addEventListener(
    "pointerlockchange",
    () => {
      const was = input.pointerWasLocked;
      const interruptedGesture = input.dragging;
      if (input.pointerLockChanged(document.pointerLockElement === canvas))
        document.exitPointerLock();
      if (was && interruptedGesture && document.pointerLockElement !== canvas) cancelKeyboard();
      if (!input.dragging) end();
    },
    options,
  );
  canvas.addEventListener(
    "wheel",
    (e) => {
      if (draggingSpell.get() || error.get() || !isEngineReady(engine.get()) || blocksGameplay())
        return;
      e.preventDefault();
      input.wheel(e.deltaY);
    },
    { ...options, passive: false },
  );
  return {
    end,
    dispose() {
      controller.abort();
      end();
    },
  };
}
