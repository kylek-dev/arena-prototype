import { adaptSnapshot, needsImmediateDelivery } from "../bridge/snapshot";
import { loadEngine } from "../bridge/loader";
import { readGraphics } from "../state/graphics";
import { readWeapons } from "../state/weapons";
import { readLighting } from "../state/lighting";
import { loadStartupMap, onMapStatus, readMap } from "../state/map-editor";
import {
  beforeMatch,
  customizerVisible,
  enterArena,
  onAppearances,
  readAppearance,
  readPreview,
  remoteAppearances,
} from "../state/appearance";
import { observeCastResult } from "./cast-feedback";
import {
  customLobbyVisible,
  engine,
  error,
  focus,
  interaction,
  nameplateScene,
} from "../state/session";
import { reconcileFocus } from "../domain/units/selection";
import { installPointer } from "./pointer";
import { installUnitHover } from "./unit-hover";
import { interactUnit } from "./commands";
import { installInput } from "./input-router";
import { installSpellDragging } from "./spell-drag";
import { debugEnabled, measureFrames, metrics, recordError, recordState } from "./diagnostics";
import { browserLink, installJoinLinks, takeJoinFragment } from "./network";
import { browserQueue, installPartyLinks } from "./matchmaking";
import { browserAccount } from "./account";
import { accountService, friendsService } from "../state/account";
import { browserFriends } from "./friends";
import { browserSocial, followAccount } from "./social";
import { installTelemetry } from "./telemetry";
import { matchQueue, networkLink, remoteHandles, remoteTags } from "../state/network";
import { parseHandles, parseTags, withHandles } from "../domain/handles";
import { clearPostMatchScoreboard, syncPostMatchScoreboard } from "../state/scoreboard";
export function startRuntime(canvas: HTMLCanvasElement) {
  const pointer = installPointer(canvas, () => router.cancel());
  const router = installInput(canvas, pointer.end);
  const stopSpellDrop = installSpellDragging(canvas);
  const unitHover = installUnitHover(canvas);
  const telemetry = installTelemetry(canvas);
  const fail = (message: string, cause?: unknown) => {
    // Every failure is reported, even after the first one is shown.
    telemetry.failure(cause, message);
    if (error.get()) return;
    recordError(message);
    error.set(
      `${message} Try a current desktop browser with WebGL2 enabled. See the console for details.`,
    );
    router.cancel();
  };
  window.addEventListener("error", (e) => fail(e.message));
  window.addEventListener("unhandledrejection", (e) => fail(String(e.reason?.message ?? e.reason)));
  const link = browserLink();
  networkLink.set(link);
  // A seat found by the queue is joined like any other, leaving the home
  // screen if the player is still there.
  const queue = browserQueue((ticket) => {
    clearPostMatchScoreboard();
    customLobbyVisible.set(false);
    interaction.set({ panel: null, quick: false, layout: false, hovered: null, capture: null });
    if (beforeMatch.get() || customizerVisible.get()) enterArena(() => {});
    link.join(ticket);
  });
  matchQueue.set(queue);
  // Who is logged in, if this door offers login (ADR 0055).
  const { account, stop: stopAccount } = browserAccount();
  accountService.set(account);
  const friends = browserFriends();
  friendsService.set(friends);
  const stopSocial = followAccount(account, queue, friends, browserSocial());
  void account.refresh();
  const stopPartyLinks = installPartyLinks(queue);
  // A join link wins; otherwise a reload rejoins this tab's match.
  const joining = takeJoinFragment() ?? link.resumable();
  if (joining) link.join(joining);
  const stopJoinLinks = installJoinLinks(link);
  let lastDelivery = 0;
  window.arenaShell = {
    readNetwork: link.read,
    sendNetwork: link.send,
    onNetwork: link.onStatus,
    onAppearances,
    onHandles: (raw) => remoteHandles.set(parseHandles(raw)),
    onTags: (raw) => remoteTags.set(parseTags(raw)),
    readInput: router.sample,
    readHover: unitHover.read,
    readGraphics,
    readWeapons,
    readLighting,
    readMap,
    readAppearance,
    readPreview,
    onMapStatus,
    onFocusUnit: (tag) => interactUnit(tag, "focus"),
    onState(raw) {
      try {
        // Fighters are named by their relayed handles in a network match.
        const next = withHandles(adaptSnapshot(raw), remoteHandles.get()),
          previous = engine.get();
        recordState(next.frames);
        observeCastResult(next);
        nameplateScene.set(next);
        const currentFocus = focus.get();
        if (currentFocus !== null) {
          focus.set(
            reconcileFocus(
              currentFocus,
              next.roster ?? [next.player, next.target],
              !!previous && (next.tick < previous.tick || next.teamSize !== previous.teamSize),
            ),
          );
        }
        // Deliver cooldown edges immediately so starting/finishing never waits
        // for the ordinary HUD throttle. In-between snapshots animate in rAF.
        const now = performance.now();
        if (now - lastDelivery < 80 && !needsImmediateDelivery(previous, next)) return;
        lastDelivery = now;
        engine.set(next);
        syncPostMatchScoreboard(next, link.state.get());
        if (!previous && document.activeElement === document.body) canvas.focus();
      } catch (e) {
        fail(String(e), e);
      }
    },
  };
  if (debugEnabled)
    window.arenaDebug = {
      get state() {
        return structuredClone(engine.get());
      },
      get metrics() {
        return metrics();
      },
    };
  const stopFrameTiming = measureFrames(() => (engine.get()?.frames ?? 0) > 30);
  // The arena starts from the map embedded at build time; the file (which the
  // editor may have saved since, or the map last opened on this device)
  // replaces it as soon as it arrives.
  void loadStartupMap();
  loadEngine().catch((e) => {
    console.error(e);
    fail(String(e.message ?? e), e);
  });
  return () => {
    telemetry.stop();
    stopJoinLinks();
    stopPartyLinks();
    queue.dispose();
    matchQueue.set(null);
    stopSocial();
    stopAccount();
    accountService.set(null);
    friendsService.set(null);
    link.dispose();
    networkLink.set(null);
    remoteAppearances.set(null);
    remoteHandles.set(null);
    remoteTags.set(null);
    clearPostMatchScoreboard();
    customLobbyVisible.set(false);
    stopFrameTiming();
    router.dispose();
    pointer.dispose();
    stopSpellDrop();
    unitHover.dispose();
    nameplateScene.set(null);
  };
}
