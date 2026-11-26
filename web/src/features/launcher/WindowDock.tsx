import { draggingSpell, engine, interaction, setPanel } from "../../state/session";
import { useStore } from "../../state/store";
import { friendsWindowVisible, playWindowVisible } from "../../state/windows";
import { DockWindow } from "../../ui/DockWindow";
import { Spellbook } from "../spellbook/Spellbook";
import { FriendsWindow } from "./FriendsWindow";
import { PlayWindow } from "./PlayWindow";
import "../../ui/dock.css";

/** The window dock (playtest inbox): Spells, Friends and Play open here, at
 * the left middle of the screen, each the same size, side by side in that
 * order when several are open. */
export function WindowDock() {
  const spells = useStore(interaction, (s) => s.panel === "spellbook");
  const spectating = useStore(engine, (s) => Boolean(s?.spectating));
  const dragging = useStore(draggingSpell, (d) => d);
  const friends = useStore(friendsWindowVisible, (v) => v);
  const play = useStore(playWindowVisible, (v) => v);
  const showSpells = spells && !spectating;
  if (!showSpells && !friends && !play) return null;
  return (
    <div className="window-dock">
      {showSpells && (
        <DockWindow
          title="Spells"
          keyboardOwner={false}
          close={() => setPanel(null)}
          // A dragged spell drops onto the action bars through the window.
          className={`spells-window ${dragging ? "drag-through" : ""}`}
        >
          <Spellbook />
        </DockWindow>
      )}
      {friends && <FriendsWindow />}
      {play && <PlayWindow />}
    </div>
  );
}
