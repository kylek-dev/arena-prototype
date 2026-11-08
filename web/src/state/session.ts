import type { Snapshot } from "../bridge/snapshot";
import { createStore } from "./store";
export const engine = createStore<Snapshot | null>(null);
// Render-rate presentation, separate from the throttled ordinary HUD snapshot.
export const nameplateScene = createStore<Snapshot | null>(null);
export const focus = createStore<string | null>(null);
export const draggingSpell = createStore(false);
export const pressingSpell = createStore(false);
// Input feedback only; independent of spell assignments and combat availability.
export const heldSlots = createStore<ReadonlySet<string>>(new Set());
export const spellDragPreview = createStore<{ x: number; y: number; target: string | null } | null>(
  null,
);
export type Panel = "menu" | "settings" | "spellbook" | null;
export const selectedLayout = createStore<string | null>(null);
export const interaction = createStore({
  panel: null as Panel,
  quick: false,
  layout: false,
  hovered: null as string | null,
  capture: null as { action: string; entry: 0 | 1 } | null,
});
export const error = createStore("");
export function blocksGameplay() {
  const mode = interaction.get();
  return (
    !!mode.capture ||
    mode.quick ||
    mode.layout ||
    mode.panel === "menu" ||
    mode.panel === "settings"
  );
}
export function setPanel(panel: Panel) {
  interaction.set({
    ...interaction.get(),
    panel,
    quick: false,
    layout: false,
    capture: null,
    hovered: null,
  });
}
export function setMode(mode: "quick" | "layout") {
  const s = interaction.get();
  interaction.set({
    panel: null,
    quick: mode === "quick" ? !s.quick : false,
    layout: mode === "layout" ? !s.layout : false,
    hovered: null,
    capture: null,
  });
}
