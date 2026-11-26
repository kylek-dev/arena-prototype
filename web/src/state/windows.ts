import { createStore } from "./store";
// The in-game windows the launcher opens (P Play, F Friends); S Spells and
// C Customize use the spellbook panel and the customizer.
export const playWindowVisible = createStore(false);
export const friendsWindowVisible = createStore(false);
/** Closes both; `true` when one was open (Escape closes them first). */
export function closeLauncherWindows() {
  const open = playWindowVisible.get() || friendsWindowVisible.get();
  playWindowVisible.set(false);
  friendsWindowVisible.set(false);
  return open;
}
