import { InputState } from "../../input.js";
import { availability, canActivate, isEngineReady } from "../domain/abilities/catalog";
import { isAbilityId } from "../domain/action-bars/model";
import { beginCast, clearCastFeedback, rejectCast } from "./cast-feedback";
import { nameplateNotice, setNameplates } from "./nameplate-feedback";
import { nextTarget } from "../domain/units/selection";
import { resolveUnit, type UnitTag, type UnitRoster } from "../domain/units/tags";
import type { Snapshot } from "../bridge/snapshot";
import { config } from "../state/config";
import { drawWeapon, toggleSheath } from "../state/weapons";
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
export function setSpectatorCamera(target: "free" | number) {
  const s = engine.get();
  if (!s?.spectating || error.get() || blocksGameplay()) return;
  if (target === "free") {
    input.requestSpectatorCamera("free");
    return;
  }
  const count = s.roster?.length ?? 0;
  if (Number.isInteger(target) && target >= 0 && target < count) {
    input.requestSpectatorCamera(target);
  }
}
export function cycleSpectatorCamera(step: 1 | -1) {
  const s = engine.get();
  if (!s?.spectating || error.get() || blocksGameplay()) return;
  const count = s.roster?.length ?? 0;
  if (count <= 0) return;
  const current = s.spectatorFollow ?? null;
  const next = current === null ? (step > 0 ? 0 : count - 1) : (current + step + count) % count;
  input.requestSpectatorCamera(next);
}
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
  if (!s || s.spectating) return;
  const unit = resolveUnit(tag, unitRoster(s, focus.get()));
  if (unit) interactEntity(unit.id, action);
}
// Local practice identities only; focus never changes selection as a side effect.
export function interactEntity(id: string, action: "target" | "focus") {
  const s = engine.get();
  if (draggingSpell.get() || error.get() || blocksGameplay() || !isEngineReady(s)) return;
  if (!s || s.spectating) return;
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
  if (!slot?.ability || !isAbilityId(slot.ability) || !isEngineReady(state) || state?.spectating)
    return;
  if (!canActivate(state, slot.ability)) {
    rejectCast(availability(state, slot.ability));
    return;
  }
  drawWeapon(); // fighting draws the sword from the back
  beginCast(state, slot.ability);
  input.requestCast(slot.ability);
}
/** Whether the local player has an active cast (never while spectating). */
export function isCasting(s: Snapshot | null = engine.get()) {
  return !!s && !s.spectating && isEngineReady(s) && !!s.player.cast;
}

/** How long after a stop request Escape stops intercepting (about one round trip). */
export const STOP_CAST_GRACE_MS = 500;
let lastStopAt = Number.NEGATIVE_INFINITY;

/** Whether a stop was requested within {@link STOP_CAST_GRACE_MS}. */
export function stopRecentlyRequested(now = performance.now()) {
  return now - lastStopAt < STOP_CAST_GRACE_MS;
}

/** Cancels the local player's active cast on the next tick (a no-op when idle). */
export function stopCasting() {
  if (!isCasting()) return;
  input.requestStopCast();
  lastStopAt = performance.now();
}

export function dispatch(action: string) {
  const interfaceAction = ["menu", "settings", "spellbook", "quickBind", "layout"].includes(action);
  if (!interfaceAction && (draggingSpell.get() || error.get() || blocksGameplay())) return;
  const s = engine.get();
  if (!interfaceAction && !isEngineReady(s)) return;
  if (action.startsWith("slot:")) {
    if (!s?.spectating) activateSlot(action.slice(5));
    return;
  }
  if (s?.spectating) {
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
      case "nextTarget":
        cycleSpectatorCamera(1);
        break;
      case "clearTarget":
        setSpectatorCamera("free");
        break;
      case "menu":
        setPanel(interaction.get().panel === "menu" ? null : "menu");
        break;
      case "settings":
        setPanel(interaction.get().panel === "settings" ? null : "settings");
        break;
    }
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
    case "stopCasting":
      stopCasting();
      break;
    case "toggleSheath":
      toggleSheath();
      break;
    case "autorun":
      input.autorun = !input.autorun;
      break;
    case "forward":
    case "back":
      input.autorun = false;
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
