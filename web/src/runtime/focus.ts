import { blocksGameplay, draggingSpell, pressingSpell } from "../state/session";

export function isTextEntry(element: Element | null): boolean {
  return !!element?.closest(
    'input, textarea, select, [contenteditable]:not([contenteditable="false"]), [role="textbox"]',
  );
}

export function needsKeyboard(element: Element | null): boolean {
  return isTextEntry(element) || !!element?.closest('[data-keyboard-owner="true"]');
}

// Ordinary HUD clicks return focus to gameplay. Explicit keyboard navigation
// opts out until the next pointer interaction, so focus is never a keyboard trap.
export function installFocusPolicy(
  canvas: HTMLCanvasElement,
  signal: AbortSignal,
  suspend: () => void,
) {
  let navigating = false;

  function restore() {
    if (
      !signal.aborted &&
      !navigating &&
      !blocksGameplay() &&
      !draggingSpell.get() &&
      !pressingSpell.get() &&
      !document.hidden &&
      document.hasFocus() &&
      !needsKeyboard(document.activeElement)
    ) {
      canvas.focus({ preventScroll: true });
    }
  }

  const deferRestore = () => queueMicrotask(restore);
  document.addEventListener(
    "focusin",
    () => {
      if (document.activeElement === canvas) navigating = false;
      if (needsKeyboard(document.activeElement) || blocksGameplay()) suspend();
      else deferRestore();
    },
    { signal },
  );
  document.addEventListener("focusout", deferRestore, { signal });
  document.addEventListener(
    "pointerdown",
    () => {
      navigating = false;
    },
    { signal },
  );
  document.addEventListener("click", deferRestore, { signal });
  canvas.addEventListener("blur", deferRestore, { signal });
  window.addEventListener("focus", deferRestore, { signal });
  document.addEventListener(
    "visibilitychange",
    () => {
      if (!document.hidden) deferRestore();
    },
    { signal },
  );

  return {
    restore: deferRestore,
    canRoute() {
      return !navigating && !needsKeyboard(document.activeElement);
    },
    leave() {
      navigating = true;
      document.getElementById("menu-access")?.focus();
    },
  };
}
