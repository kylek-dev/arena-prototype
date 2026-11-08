import { InputState } from "../../input.js";
import { availability, canActivate, isEngineReady } from "../domain/abilities/catalog";
import { beginCast, clearCastFeedback, rejectCast } from "./cast-feedback";
import { nameplateNotice, setNameplates } from "./nameplate-feedback";
import { nextTarget } from "../domain/units/selection";
import { resolveUnit, type UnitTag, type UnitRoster } from "../domain/units/tags";
import type { Snapshot } from "../bridge/snapshot";
import { config } from "../state/config";
import {
  draggingSpell,
  blocksGameplay,
  engine,
  error,
  focus,
  interaction,
  inputReset,
  setMode,
  setPanel,
} from "../state/session";
export const input = new InputState();
export function unitRoster(s: Snapshot, focused: string | null): UnitRoster {
  return {
    player: s.player,
    target: s.roster
      ? (s.roster.find((unit) => unit.actor === s.selectedActor) ?? null)
      : s.selected
        ? s.selectedPlayer
          ? s.player
          : s.target
        : null,
    focus: focused,
    opponents: s.roster?.filter((unit) => unit.tag.startsWith("Opponent")) ?? [s.target],
    team: s.roster?.filter((unit) => unit.tag.startsWith("Team") && unit.id !== s.player.id) ?? [],
  };
}
export function interactUnit(tag: UnitTag, action: "target" | "focus") {
  const s = engine.get();
  if (!s) return;
  const unit = resolveUnit(tag, unitRoster(s, focus.get()));
  if (unit) interactEntity(unit.id, action);
}
// Local practice identities only; focus never changes selection as a side effect.
export function interactEntity(id: string, action: "target" | "focus") {
  const s = engine.get();
  if (draggingSpell.get() || error.get() || blocksGameplay() || !isEngineReady(s)) return;
  if (!s) return;
  const unit = s.roster?.find((unit) => unit.id === id);
  if (s.roster ? !unit : s.target.id !== id && s.player.id !== id) return;
  if (action === "focus") focus.set(id);
  else {
    if (unit) {
      input.selectActor = unit.actor;
      return;
    }
    input.select = s.target.id === id;
    input.selectPlayer = s.player.id === id;
  }
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
    case "toggleEnemyNameplates":
    case "toggleFriendlyNameplates": {
      const side = action === "toggleEnemyNameplates" ? "enemy" : "friendly";
      setNameplates(side, !config().nameplates[side]);
      break;
    }
    case "jump":
      input.jump = true;
      break;
    case "menu":
      setPanel(interaction.get().panel === "menu" ? null : "menu");
      break;
    case "nextTarget":
      if (s) {
        const roster = unitRoster(s, focus.get());
        const id = nextTarget(
          roster.opponents.filter((unit) => unit.health > 0).map((unit) => unit.id),
          roster.target?.id ?? null,
        );
        if (id) interactEntity(id, "target");
      }
      break;
    case "clearTarget":
      input.clear = true;
      break;
    case "setFocus":
      if (s) focus.set(unitRoster(s, focus.get()).target?.id ?? null);
      break;
    case "clearFocus":
      focus.set(null);
      break;
    case "targetFocus":
      interactUnit("Focus", "target");
      break;
    case "reset":
      inputReset.set(inputReset.get() + 1);
      nameplateNotice.set(null);
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
