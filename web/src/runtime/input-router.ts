import { BindingResolver, chord, same, unsupported } from "../domain/bindings/model";
import { config, preferences } from "../state/config";
import { captureMessage, acceptBinding, pendingBinding } from "../state/binding-capture";
import {
  blocksGameplay,
  draggingSpell,
  pressingSpell,
  engine,
  error,
  interaction,
  heldSlots,
  inputReset,
} from "../state/session";
import { isEngineReady } from "../domain/abilities/catalog";
import { dispatch, input } from "./commands";
import { installFocusPolicy, isTextEntry } from "./focus";
// Precedence: conflict > capture > quick bind > modal/typing > layout > gameplay.
export function installInput(canvas: HTMLCanvasElement, releaseMouse: () => void) {
  const resolver = new BindingResolver();
  const syncHeldSlots = () => {
    heldSlots.set(
      new Set(
        [...resolver.held.values()]
          .filter(({ action }) => action.startsWith("slot:"))
          .map(({ action }) => action.slice(5)),
      ),
    );
  };
  const events = new AbortController(),
    options = { signal: events.signal };
  const cancel = () => {
    resolver.clear();
    syncHeldSlots();
    input.cancel();
    releaseMouse();
  };
  const focusPolicy = installFocusPolicy(canvas, events.signal, cancel);
  const unReset = inputReset.subscribe(cancel);
  let previousEngine = engine.get();
  const unEngine = engine.subscribe(() => {
    const next = engine.get();
    const previous = previousEngine;
    previousEngine = next;
    if (previous && (!next || (!previous.won && next.won) || next.tick < previous.tick)) {
      cancel();
    }
  });
  let previousConfig = config();
  let previousProfile = preferences.get().active;
  const unsubscribe = interaction.subscribe(() => {
      cancel();
      focusPolicy.restore();
    }),
    unconfig = preferences.subscribe(() => {
      const next = config();
      // Visibility hotkeys are presentation-only: keep walking/steering and
      // held-key ownership intact. All other preference edits still cancel.
      const onlyNameplates =
        preferences.get().active === previousProfile &&
        next.nameplates !== previousConfig.nameplates &&
        next.bindings === previousConfig.bindings &&
        next.bars === previousConfig.bars &&
        next.frames === previousConfig.frames &&
        next.assignmentsLocked === previousConfig.assignmentsLocked;
      previousConfig = next;
      previousProfile = preferences.get().active;
      if (!onlyNameplates) cancel();
    });
  const unDrag = draggingSpell.subscribe(() => {
    cancel();
    focusPolicy.restore();
  });
  function keydown(e: KeyboardEvent) {
    if (draggingSpell.get() || pressingSpell.get()) return;
    const mode = interaction.get();
    if (pendingBinding.get()) {
      if (e.code === "Escape") {
        pendingBinding.set(null);
        e.preventDefault();
        e.stopImmediatePropagation();
      }
      return;
    }
    if (mode.capture || mode.quick) {
      if (e.code === "Tab" && !mode.capture) return; // Quick binding allows ordinary slot navigation.
      e.stopImmediatePropagation();
      if (e.code === "Escape" && mode.capture?.action !== "menu") {
        e.preventDefault();
        interaction.set({ ...mode, capture: null, quick: false, hovered: null });
        captureMessage.set("Capture cancelled.");
        return;
      }
      const destination =
        mode.capture ??
        (mode.hovered ? { action: `slot:${mode.hovered}`, entry: 0 as const } : null);
      if (!destination) return;
      const c = chord(e),
        problem = unsupported(c);
      if (problem) {
        captureMessage.set(problem);
        return;
      }
      e.preventDefault();
      if (!e.repeat) acceptBinding(destination.action, destination.entry, c);
      return;
    }
    if (mode.layout && e.code === "Escape") {
      e.preventDefault();
      interaction.set({ ...mode, layout: false });
      return;
    }
    if (
      mode.panel === "menu" &&
      !isTextEntry(document.activeElement) &&
      config().bindings.menu?.some((c) => same(c, chord(e)))
    ) {
      e.preventDefault();
      e.stopImmediatePropagation();
      if (!e.repeat) dispatch("menu");
      return;
    }
    if (blocksGameplay() || error.get() || !focusPolicy.canRoute()) return;
    if (e.code === "Tab" && e.shiftKey) {
      cancel();
      focusPolicy.leave();
      e.preventDefault();
      return;
    }
    if (!isEngineReady(engine.get())) return;
    canvas.focus({ preventScroll: true });
    const value = chord(e);
    if (
      e.repeat &&
      Object.values(config().bindings).some((pair) => pair.some((c) => same(c, value)))
    ) {
      e.preventDefault();
      return;
    }
    const action = resolver.down(e, config().bindings);
    if (!action) return;
    e.preventDefault();
    syncHeldSlots();
    dispatch(action);
  }
  window.addEventListener("keydown", keydown, { ...options, capture: true });
  window.addEventListener(
    "keyup",
    (e) => {
      resolver.up(e.code, e.shiftKey);
      syncHeldSlots();
    },
    options,
  );
  window.addEventListener("blur", cancel, options);
  document.addEventListener(
    "visibilitychange",
    () => {
      if (document.hidden) cancel();
    },
    options,
  );
  return {
    cancel,
    sample() {
      const result = input.readInput();
      result[0] = Number(resolver.active("right")) - Number(resolver.active("left"));
      result[1] =
        input.mouseButtons === 3
          ? -1
          : Number(resolver.active("back")) - Number(resolver.active("forward"));
      return result;
    },
    dispose() {
      events.abort();
      unsubscribe();
      unconfig();
      unDrag();
      unReset();
      unEngine();
      cancel();
    },
  };
}
