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
  customizing,
  prematch,
  scoreboardVisible,
  customLobbyVisible,
  setPanel,
} from "../state/session";
import { isEngineReady } from "../domain/abilities/catalog";
import { escapeStopsCast } from "../domain/bindings/escape";
import { dispatch, input, isCasting, stopCasting, stopRecentlyRequested } from "./commands";
import { installFocusPolicy, isTextEntry } from "./focus";
import { chatShown, requestChat } from "../state/chat";
import { closeLauncherWindows } from "../state/windows";
import { popupMenu } from "../state/popup-menu";
// Precedence: conflict > capture > quick bind > text entry > Escape stops a cast >
// modal/typing > layout > gameplay (see domain/bindings/escape.ts).
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
  // Opening the character close-up releases held movement like a menu does.
  const unCustomize = customizing.subscribe(() => {
    cancel();
    focusPolicy.restore();
  });
  const unPrematch = prematch.subscribe(() => {
    cancel();
    focusPolicy.restore();
  });
  const unScoreboard = scoreboardVisible.subscribe(() => {
    cancel();
    focusPolicy.restore();
  });
  const unCustomLobby = customLobbyVisible.subscribe(() => {
    cancel();
    focusPolicy.restore();
  });
  const unDrag = draggingSpell.subscribe(() => {
    cancel();
    focusPolicy.restore();
  });
  function keydown(e: KeyboardEvent) {
    if (draggingSpell.get() || pressingSpell.get()) return;
    const mode = interaction.get();
    // A plain Escape stops the current cast before it closes or opens anything.
    if (
      e.code === "Escape" &&
      !e.shiftKey &&
      !e.ctrlKey &&
      !e.altKey &&
      !e.metaKey &&
      escapeStopsCast({
        capturing: !!pendingBinding.get() || !!mode.capture || mode.quick,
        textEntry: isTextEntry(document.activeElement),
        // Right after a stop, a second Escape goes on to the panels/menu
        // even before the server's snapshot confirms the cast ended.
        casting: !error.get() && isCasting() && !stopRecentlyRequested(),
      })
    ) {
      e.preventDefault();
      e.stopImmediatePropagation();
      if (!e.repeat) stopCasting();
      return;
    }
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
    // An open menu over the game (which leaves the keys with the game)
    // closes on Escape before the windows or the game menu.
    const menu = popupMenu.get();
    if (e.code === "Escape" && menu && !isTextEntry(document.activeElement)) {
      e.preventDefault();
      if (!e.repeat) menu();
      return;
    }
    if (
      customLobbyVisible.get() &&
      !scoreboardVisible.get() &&
      e.code === "Escape" &&
      !mode.panel
    ) {
      e.preventDefault();
      customLobbyVisible.set(false);
      return;
    }
    // An open menu inside a window takes its own Escape.
    if (e.code === "Escape" && document.activeElement?.closest('[data-escape-owner="true"]'))
      return;
    // Escape closes the dock's windows (Spells, Friends, Play) first.
    if (e.code === "Escape" && mode.panel === "spellbook" && !isTextEntry(document.activeElement)) {
      e.preventDefault();
      setPanel(null);
      return;
    }
    if (
      e.code === "Escape" &&
      !mode.panel &&
      !customizing.get() &&
      !prematch.get() &&
      !scoreboardVisible.get() &&
      !isTextEntry(document.activeElement) &&
      closeLauncherWindows()
    ) {
      e.preventDefault();
      return;
    }
    if (customizing.get() && e.code === "Escape" && !mode.panel) {
      e.preventDefault();
      customizing.set(false);
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
    // Enter opens chat, and `/` opens it with a command started (unless
    // `/` is bound to an action); while typing there, keys go to the chat box.
    const plain = !e.shiftKey && !e.ctrlKey && !e.altKey && !e.metaKey;
    // `/` by its character (any layout) or the numpad; only when no action
    // is bound to that key.
    const slash =
      (e.key === "/" || e.code === "Slash" || e.code === "NumpadDivide") &&
      !e.ctrlKey &&
      !e.altKey &&
      !e.metaKey &&
      !Object.values(config().bindings).some((pair) => pair.some((c) => same(c, chord(e))));
    if (((e.code === "Enter" || e.code === "NumpadEnter") && plain) || slash) {
      if (chatShown.get()) {
        e.preventDefault();
        e.stopImmediatePropagation();
        // A held key opens the field once, never toggles it.
        if (e.repeat) return;
        cancel();
        requestChat(slash ? "/" : "");
        return;
      }
    }
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
          : Number(resolver.active("back")) - Number(resolver.active("forward") || input.autorun);
      if (engine.get()?.spectating && resolver.active("jump")) result[14] = 1;
      return result;
    },
    dispose() {
      events.abort();
      unsubscribe();
      unconfig();
      unDrag();
      unCustomize();
      unPrematch();
      unScoreboard();
      unCustomLobby();
      unReset();
      unEngine();
      cancel();
    },
  };
}
