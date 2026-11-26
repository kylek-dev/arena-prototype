import { useEffect } from "react";
import { TooltipProvider } from "../ui/Tooltip";
import { Theme } from "@radix-ui/themes";
import { Frames } from "../features/unit-frames/Frames";
import { PlayerCastBar } from "../features/cast-bar/PlayerCastBar";
import { Nameplates } from "../features/nameplates/Nameplates";
import { CombatText } from "../features/combat-text/CombatText";
import { UnitTooltip } from "../features/unit-frames/UnitTooltip";
import { ActionBars } from "../features/action-bars/ActionBars";
import { SpellDragPreview } from "../features/spellbook/SpellDragPreview";
import { Settings } from "../features/settings/Settings";
import { GameMenu } from "../features/menu/GameMenu";
import { QuickBinding } from "../features/keybindings/QuickBinding";
import { BindingConflict } from "../features/keybindings/BindingsEditor";
import { LayoutToolbar } from "../features/layout-editor/LayoutSettings";
import { CombatErrors } from "../features/combat-errors/CombatErrors";
import { MatchStatus } from "../features/match-status/MatchStatus";
import { RoundClock } from "../features/round-clock/RoundClock";
import { Scoreboard } from "../features/scoreboard/Scoreboard";
import { CustomLobby } from "../features/custom-lobby/CustomLobby";
import { SpectatorHud } from "../features/spectator/SpectatorHud";
import { MatchReady } from "../features/queue/MatchReady";
import { InvitePrompt } from "../features/friends/InvitePrompt";
import { ChatBox } from "../features/chat/ChatBox";
import { Launcher } from "../features/launcher/Launcher";
import { CenterPanel } from "../features/center/CenterPanel";
import { WindowDock } from "../features/launcher/WindowDock";
import { QueueBar } from "../features/queue/QueueBar";
import { Loading } from "../features/loading/Loading";
import { Diagnostics } from "../features/diagnostics/Diagnostics";
import { MapEditor } from "../features/map-editor/MapEditor";
import { HubNameplates } from "../features/hub/HubNameplates";
import { OUT_OF_HUB } from "../domain/hub";
import { matchQueue } from "../state/network";
import { Customizer } from "../features/customizer/Customizer";
import { beforeMatch, customizerVisible } from "../state/appearance";
import { LauncherPage } from "../features/launcher-page/LauncherPage";
import { Modal } from "../ui/Dialog";
import {
  customLobbyVisible,
  draggingSpell,
  engine,
  interaction,
  scoreboardVisible,
  setPanel,
} from "../state/session";
import { closeLauncherWindows } from "../state/windows";
import { saveMessage, useConfigSelector } from "../state/config";
import { useStore } from "../state/store";
import { pendingBinding } from "../state/binding-capture";
import { label } from "../domain/bindings/model";
import { createStore } from "../state/store";

const NO_HUB = createStore(OUT_OF_HUB);

export function App() {
  const mode = useStore(interaction, (s) => s);
  const save = useStore(saveMessage, (s) => s);
  const pending = useStore(pendingBinding, (s) => s);
  const dragging = useStore(draggingSpell, (s) => s);
  const customizing = useStore(customizerVisible, (s) => s);
  const lobby = useStore(beforeMatch, (s) => s);
  const spectating = useStore(engine, (s) => Boolean(s?.spectating));
  const lobbyOpen = useStore(customLobbyVisible, (v) => v);
  const boardOpen = useStore(scoreboardVisible, (v) => v);
  // The hub is a social place (ADR 0056): no combat HUD there.
  const queue = useStore(matchQueue, (q) => q);
  const inHub = useStore(queue?.hub ?? NO_HUB, (h) => h.status === "in");
  // The Play and Friends windows belong to the arena: a full-screen overlay
  // (customizer, launcher page, custom lobby, scoreboard) closes them.
  useEffect(() => {
    if (customizing || lobby || lobbyOpen || boardOpen) closeLauncherWindows();
  }, [customizing, lobby, lobbyOpen, boardOpen]);
  useEffect(() => {
    if (!spectating) return;
    const current = interaction.get();
    if (current.panel === "spellbook" || current.layout || current.quick || current.capture) {
      interaction.set({
        ...current,
        panel: current.panel === "spellbook" ? null : current.panel,
        layout: false,
        quick: false,
        capture: null,
      });
    }
  }, [spectating]);
  const menuBindings = useConfigSelector((c) => c.bindings.menu);
  const menuKey = menuBindings?.find(Boolean);
  const title = mode.panel === "menu" ? "Game menu" : "Settings";

  return (
    <Theme
      id="overlay-theme"
      className="overlay-theme"
      appearance="dark"
      accentColor="indigo"
      grayColor="slate"
      radius="medium"
      panelBackground="solid"
    >
      <TooltipProvider disabled={dragging || mode.quick || mode.layout || !!mode.capture}>
        {save && (
          <p className="save-notice" role="alert">
            {save} Open Menu → Settings → Profiles to export or recover.
          </p>
        )}
        {/* The close-up hides the combat HUD so the character is unobstructed. */}
        {!customizing && !lobby && (
          <>
            <button id="menu-access" className="menu-access" onClick={() => setPanel("menu")}>
              Open game menu
            </button>
            <span className="menu-hint">{menuKey ? label(menuKey) : "Shift+Tab"} · Menu</span>
            {inHub ? (
              <HubNameplates />
            ) : (
              <>
                <Nameplates />
                <CombatText />
              </>
            )}
            {inHub ? null : spectating ? (
              <SpectatorHud />
            ) : (
              <>
                <Frames />
                <PlayerCastBar />
                <UnitTooltip />
                <ActionBars />
                <SpellDragPreview />
                <CombatErrors />
                <QuickBinding />
                <LayoutToolbar />
              </>
            )}
            {!inHub && (
              <>
                <RoundClock />
                <MatchStatus />
              </>
            )}
            {/* The launcher first: the chat measures it to sit beside it. */}
            <Launcher />
            <ChatBox />
          </>
        )}
        <Diagnostics />
        <Scoreboard />
        <CustomLobby />
        {/* The queue follows the player from the launcher page into the arena. */}
        <CenterPanel>
          <MatchReady />
          <QueueBar />
          <InvitePrompt />
        </CenterPanel>
        <WindowDock />
        <MapEditor />
        <LauncherPage />
        <Customizer />
        <Loading />
        {(mode.panel === "menu" || mode.panel === "settings") && (
          <Modal
            key={mode.panel}
            title={title}
            modal
            className={mode.panel === "menu" ? "menu-dialog" : ""}
            escapeBlocked={!!mode.capture || !!pending}
            close={() => setPanel(null)}
          >
            {mode.panel === "menu" ? <GameMenu /> : <Settings />}
          </Modal>
        )}
        <BindingConflict />
      </TooltipProvider>
    </Theme>
  );
}
