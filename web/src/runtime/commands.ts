import { InputState } from "../../input.js";
import { canActivate, isEngineReady } from "../domain/abilities/catalog";
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
export function activateSlot(id: string) {
  if (draggingSpell.get() || error.get() || blocksGameplay()) return;
  const slot = config()
    .bars.flatMap((b) => b.slots)
    .find((s) => s.id === id);
  if (slot?.ability === "pulse" && canActivate(engine.get())) input.requestAttack();
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
