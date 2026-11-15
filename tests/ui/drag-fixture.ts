import { fireEvent } from "@testing-library/react";
import { vi } from "vitest";
import type { Placement } from "../../web/src/domain/action-bars/model";
import { beginSpellPress } from "../../web/src/runtime/spell-drag";

export function armSpell(source: HTMLElement, assignment: Placement, shiftKey = false) {
  beginSpellPress(
    {
      currentTarget: source,
      pointerId: 1,
      button: 0,
      clientX: 100,
      clientY: 100,
      shiftKey,
      preventDefault() {},
    },
    assignment,
  );
}

// jsdom has no rendered hit testing. Return only the target the test specifies;
// all gesture thresholds, transactions and cancellation use production code.
export function hitTarget(target: Element | null) {
  Object.defineProperty(document, "elementFromPoint", {
    configurable: true,
    value: vi.fn(() => target),
  });
}

export function moveSpell(x = 200, y = 200) {
  fireEvent.pointerMove(document, { pointerId: 1, clientX: x, clientY: y, buttons: 1 });
}

export function dropSpell(shiftKey = false, x = 200, y = 200) {
  fireEvent.pointerUp(document, { pointerId: 1, clientX: x, clientY: y, shiftKey });
}
