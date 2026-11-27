import { createStore } from "./store";

/** The small menu open over the game (a unit's right-click menu, a friend
 * row's ⋮ menu): how to close it. These menus never take the keyboard from
 * the game (keys keep moving and casting; items are clicked), so the input
 * router closes the open one on Escape before anything else uses the key. */
export const popupMenu = createStore<(() => void) | null>(null);

/** Registers `close` as the open menu's; returns the function that clears
 * it again (only if it is still this menu's). */
export function holdPopupMenu(close: () => void) {
  popupMenu.get()?.();
  popupMenu.set(close);
  return () => {
    if (popupMenu.get() === close) popupMenu.set(null);
  };
}
