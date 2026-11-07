import { place, removeAssignment, type Placement } from "../domain/action-bars/model";
import { blocksGameplay, draggingSpell, pressingSpell, spellDragPreview } from "../state/session";
import { config, preferences, updateConfig } from "../state/config";

type SpellPress = {
  pointerId: number;
  source: HTMLElement;
  profile: string;
  x: number;
  y: number;
  assignment: Placement;
};

let press: SpellPress | null = null;
let suppressClick = false;
const DRAG_THRESHOLD = 4;

// Pointer capture keeps the gesture on the source even over the Bevy canvas.
// Do not depend on native HTML dragstart or steal focus during an armed press.
type SpellPointerPress = Pick<
  PointerEvent,
  "button" | "pointerId" | "clientX" | "clientY" | "shiftKey" | "preventDefault"
> & { currentTarget: HTMLElement };

export function beginSpellPress(event: SpellPointerPress, assignment: Placement) {
  if (event.button !== 0 || config().assignmentsLocked || blocksGameplay()) return;
  event.preventDefault();
  endDrag();
  suppressClick = false;
  press = {
    pointerId: event.pointerId,
    source: event.currentTarget,
    profile: preferences.get().active,
    x: event.clientX,
    y: event.clientY,
    assignment: { ...assignment, copy: event.shiftKey },
  };
  pressingSpell.set(true);
  event.currentTarget.setPointerCapture(event.pointerId);
}

export function endDrag() {
  const previous = press;
  press = null;
  pressingSpell.set(false);
  spellDragPreview.set(null);
  draggingSpell.set(false);
  if (previous?.source.hasPointerCapture?.(previous.pointerId)) {
    previous.source.releasePointerCapture(previous.pointerId);
  }
}

function targetAt(x: number, y: number) {
  return document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-spell-drop-slot], canvas");
}

export function installSpellDragging(canvas: HTMLCanvasElement) {
  const controller = new AbortController();
  const options = { signal: controller.signal, capture: true };

  function cancel() {
    if (press) suppressClick = true;
    endDrag();
  }

  document.addEventListener(
    "pointermove",
    (event) => {
      if (!press || press.pointerId !== event.pointerId) return;
      if (
        !draggingSpell.get() &&
        Math.hypot(event.clientX - press.x, event.clientY - press.y) < DRAG_THRESHOLD
      )
        return;
      event.preventDefault();
      draggingSpell.set(true);
      suppressClick = true;
      const target = targetAt(event.clientX, event.clientY);
      spellDragPreview.set({
        x: event.clientX,
        y: event.clientY,
        target: target?.dataset.spellDropSlot ?? null,
      });
    },
    options,
  );

  document.addEventListener(
    "pointerup",
    (event) => {
      const current = press;
      if (!current || current.pointerId !== event.pointerId) return;
      // A coalesced/very fast gesture may reach pointerup without a move event.
      if (
        draggingSpell.get() ||
        Math.hypot(event.clientX - current.x, event.clientY - current.y) >= DRAG_THRESHOLD
      ) {
        event.preventDefault();
        draggingSpell.set(true);
        suppressClick = true;
        const target = targetAt(event.clientX, event.clientY);
        if (
          !blocksGameplay() &&
          !config().assignmentsLocked &&
          preferences.get().active === current.profile
        ) {
          const slot = target?.dataset.spellDropSlot;
          if (slot) {
            updateConfig((c) => {
              const bars = place(c.bars, slot, {
                ...current.assignment,
                copy: current.assignment.copy || event.shiftKey,
              });
              return bars === c.bars ? c : { ...c, bars };
            });
          } else if (target === canvas && current.assignment.source) {
            updateConfig((c) => {
              const bars = removeAssignment(
                c.bars,
                current.assignment.source!,
                current.assignment.ability,
              );
              return bars === c.bars ? c : { ...c, bars };
            });
          }
        }
      }
      endDrag();
    },
    options,
  );

  document.addEventListener(
    "pointercancel",
    (event) => {
      if (press?.pointerId === event.pointerId) cancel();
    },
    options,
  );
  document.addEventListener(
    "lostpointercapture",
    (event) => {
      if (press?.pointerId === event.pointerId) cancel();
    },
    options,
  );
  document.addEventListener(
    "pointerdown",
    () => {
      suppressClick = false;
    },
    options,
  );
  window.addEventListener(
    "click",
    (event) => {
      if (!suppressClick) return;
      event.preventDefault();
      event.stopImmediatePropagation();
      suppressClick = false;
    },
    options,
  );
  window.addEventListener(
    "keydown",
    (event) => {
      if (press && event.code === "Escape") {
        event.preventDefault();
        event.stopImmediatePropagation();
        cancel();
      } else if (!press) {
        suppressClick = false;
      }
    },
    options,
  );
  window.addEventListener("blur", cancel, { signal: controller.signal });
  document.addEventListener(
    "visibilitychange",
    () => {
      if (document.hidden) cancel();
    },
    options,
  );

  return () => {
    controller.abort();
    cancel();
    suppressClick = false;
  };
}
