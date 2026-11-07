import { InputState } from "../../input.js";
import { availability, canActivate, isEngineReady } from "../domain/abilities/catalog";
import { beginCast, clearCastFeedback, rejectCast } from "./cast-feedback";
import { nextTarget } from "../domain/units/selection";
import { config } from "../state/config";
import {
  draggingSpell,
  blocksGameplay,
  engine,
  error,
  focus,
  interaction,
  setMode,
  setPanel,
} from "../state/session";
export const input = new InputState();
// The practice ABI exposes one opponent. Resolve identity before issuing intent;
// focusing is independent of selection and must not select as a side effect.
export function interactOpponent(id: string, action: "target" | "focus") {
  const s = engine.get();
  if (draggingSpell.get() || error.get() || blocksGameplay() || !isEngineReady(s)) return;
  if (!s || s.target.id !== id) return;
  if (action === "focus") focus.set(id);
  else input.select = true;
}
export function activateSlot(id: string) {
  if (draggingSpell.get() || error.get() || blocksGameplay()) return;
  const slot = config()
    .bars.flatMap((b) => b.slots)
    .find((s) => s.id === id);
  const state = engine.get();
  if (slot?.ability !== "pulse" || !isEngineReady(state)) return;
  if (!canActivate(state)) {
    rejectCast(availability(state));
    return;
  }
  beginCast(state);
  input.requestAttack();
}
export function dispatch(action: string) {
  const interfaceAction = ["menu", "settings", "spellbook", "quickBind", "layout"].includes(action);
  if (!interfaceAction && (draggingSpell.get() || error.get() || blocksGameplay())) return;
  const s = engine.get();
  if (!interfaceAction && !isEngineReady(s)) return;
  if (action.startsWith("slot:")) {
    activateSlot(action.slice(5));
    return;
  }
  switch (action) {
    case "menu":
      setPanel(interaction.get().panel === "menu" ? null : "menu");
      break;
    case "nextTarget":
      if (s && nextTarget([s.target.id], s.selected ? s.target.id : null)) input.select = true;
      break;
    case "clearTarget":
      input.clear = true;
      break;
    case "setFocus":
      if (s?.selected) focus.set(s.target.id);
      break;
    case "clearFocus":
      focus.set(null);
      break;
    case "targetFocus":
      if (s && focus.get() === s.target.id) input.select = true;
      break;
    case "reset":
      clearCastFeedback();
      focus.set(null);
      input.requestReset();
      break;
    case "settings":
      setPanel(interaction.get().panel === "settings" ? null : "settings");
      break;
    case "spellbook":
      setPanel(interaction.get().panel === "spellbook" ? null : "spellbook");
      break;
    case "quickBind":
      setMode("quick");
      break;
    case "layout":
      setMode("layout");
      break;
  }
}
