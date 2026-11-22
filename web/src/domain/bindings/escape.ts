// Whether a plain Escape press stops the current cast. Pure: no DOM, state or
// imports, so `tests/unit` can load it directly. When it does not, Escape
// keeps its existing handlers in `runtime/input-router.ts` and dialogs
// (close the top panel, then open the menu), covered by `tests/ui/menu.test.tsx`.

export type EscapeContext = {
  /** A key is being captured for a binding, or a binding conflict is pending. */
  capturing: boolean;
  /** Focus is in a text field, which owns Escape (e.g. to revert an edit). */
  textEntry: boolean;
  /** The local player has an active cast (never while spectating). */
  casting: boolean;
};

/**
 * Escape order: binding capture and text fields keep Escape; otherwise a cast
 * in progress is stopped first, ahead of any open panel or the menu.
 */
export function escapeStopsCast(context: EscapeContext): boolean {
  return !context.capturing && !context.textEntry && context.casting;
}
