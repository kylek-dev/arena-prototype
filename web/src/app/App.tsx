import * as Tooltip from "@radix-ui/react-tooltip";
import { Theme } from "@radix-ui/themes";
import { Frames } from "../features/unit-frames/Frames";
import { ActionBars } from "../features/action-bars/ActionBars";
import { Spellbook } from "../features/spellbook/Spellbook";
import { SpellDragPreview } from "../features/spellbook/SpellDragPreview";
import { Settings } from "../features/settings/Settings";
import { GameMenu } from "../features/menu/GameMenu";
import { QuickBinding } from "../features/keybindings/QuickBinding";
import { BindingConflict } from "../features/keybindings/BindingsEditor";
import { LayoutToolbar } from "../features/layout-editor/LayoutSettings";
import { Practice } from "../features/practice/Practice";
import { Loading } from "../features/loading/Loading";
import { Diagnostics } from "../features/diagnostics/Diagnostics";
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
  const menuBindings = useConfigSelector((c) => c.bindings.menu);
  const menuKey = menuBindings?.find(Boolean);
  const title =
    mode.panel === "menu" ? "Game menu" : mode.panel === "settings" ? "Settings" : "Spellbook";

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
      <Tooltip.Provider delayDuration={300}>
        <button id="menu-access" className="menu-access" onClick={() => setPanel("menu")}>
          Open game menu
        </button>
        <span className="menu-hint">{menuKey ? label(menuKey) : "Shift+Tab"} · Menu</span>
        {save && (
          <p className="save-notice" role="alert">
            {save} Open Menu → Settings → Profiles to export or recover.
          </p>
        )}
        <Frames />
        <ActionBars />
        <SpellDragPreview />
        <Practice />
        <QuickBinding />
        <LayoutToolbar />
        <Diagnostics />
        <Loading />
        {mode.panel && (
          <Modal
            key={mode.panel}
            title={title}
            description={
              mode.panel === "spellbook"
                ? "Drag a spell onto a slot. Drag a slotted spell onto the arena to remove it."
                : undefined
            }
            modal={mode.panel !== "spellbook"}
            className={
              mode.panel === "menu"
                ? "menu-dialog"
                : mode.panel === "spellbook"
                  ? `spellbook-dialog ${dragging ? "drag-through" : ""}`
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
      </Tooltip.Provider>
    </Theme>
  );
}
