import { createStore } from "./store";
// The in-game windows the launcher opens (P Play, F Friends); S Spells uses
// the spellbook panel. All of them open in the window dock (`WindowDock`).
export const playWindowVisible = createStore(false);
export const friendsWindowVisible = createStore(false);
export type PlayTab = "practice" | "queue" | "host";
/** The Play window's tab, kept while it is closed. */
export const playTab = createStore<PlayTab>("queue");
/** Opens the Play window on `tab`. */
export function openPlay(tab: PlayTab = playTab.get()) {
  playTab.set(tab);
  playWindowVisible.set(true);
}
/** Closes both; `true` when one was open (Escape closes them first). */
export function closeLauncherWindows() {
  const open = playWindowVisible.get() || friendsWindowVisible.get();
  playWindowVisible.set(false);
  friendsWindowVisible.set(false);
  return open;
}
