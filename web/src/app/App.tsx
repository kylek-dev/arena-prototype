import { TooltipProvider } from "../ui/Tooltip";
import { Theme } from "@radix-ui/themes";
import { Frames } from "../features/unit-frames/Frames";
import { Nameplates } from "../features/nameplates/Nameplates";
import { CombatText } from "../features/combat-text/CombatText";
import { UnitTooltip } from "../features/unit-frames/UnitTooltip";
import { ActionBars } from "../features/action-bars/ActionBars";
import { Spellbook } from "../features/spellbook/Spellbook";
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
import { MatchReady } from "../features/queue/MatchReady";
import { QueueBar } from "../features/queue/QueueBar";
import { Loading } from "../features/loading/Loading";
import { Diagnostics } from "../features/diagnostics/Diagnostics";
import { LightingTweaks } from "../features/lighting-tweaks/LightingTweaks";
import { MapEditor } from "../features/map-editor/MapEditor";
import { Customizer } from "../features/customizer/Customizer";
import { beforeMatch, customizerVisible } from "../state/appearance";
import { StartScreen } from "../features/customizer/StartScreen";
import { Modal } from "../ui/Dialog";
import { draggingSpell, interaction, setPanel } from "../state/session";
import { saveMessage, useConfigSelector } from "../state/config";
import { useStore } from "../state/store";
import { pendingBinding } from "../state/binding-capture";
import { label } from "../domain/bindings/model";

export function App() {
  const mode = useStore(interaction, (s) => s);
  const save = useStore(saveMessage, (s) => s);
  const pending = useStore(pendingBinding, (s) => s);
  const dragging = useStore(draggingSpell, (s) => s);
  const customizing = useStore(customizerVisible, (s) => s);
  const lobby = useStore(beforeMatch, (s) => s);
  const menuBindings = useConfigSelector((c) => c.bindings.menu);
  const menuKey = menuBindings?.find(Boolean);
  const title =
    mode.panel === "menu" ? "Game menu" : mode.panel === "settings" ? "Settings" : "Spells";

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
            <Nameplates />
            <CombatText />
            <Frames />
            <UnitTooltip />
            <ActionBars />
            <SpellDragPreview />
            <CombatErrors />
            <RoundClock />
            <MatchStatus />
            <QuickBinding />
            <LayoutToolbar />
          </>
        )}
        <Diagnostics />
        <Scoreboard />
        <CustomLobby />
        {/* The queue follows the player from the home screen into the arena. */}
        <QueueBar />
        <MatchReady />
        <LightingTweaks />
        <MapEditor />
        <StartScreen />
        <Customizer />
        <Loading />
        {mode.panel && (
          <Modal
            key={mode.panel}
            title={title}
            description={mode.panel === "spellbook" ? "Drag a spell onto a slot." : undefined}
            modal={mode.panel !== "spellbook"}
            className={
              mode.panel === "menu"
                ? "menu-dialog"
                : mode.panel === "spellbook"
                  ? `spellbook-dialog window-area-left ${dragging ? "drag-through" : ""}`
                  : ""
            }
            escapeBlocked={!!mode.capture || !!pending}
            close={() => setPanel(null)}
          >
            {mode.panel === "menu" ? (
              <GameMenu />
            ) : mode.panel === "settings" ? (
              <Settings />
            ) : (
              <Spellbook />
            )}
          </Modal>
        )}
        <BindingConflict />
      </TooltipProvider>
    </Theme>
  );
}
