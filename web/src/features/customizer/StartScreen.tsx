import { Button, Separator } from "@radix-ui/themes";
import { selectKit } from "../../domain/appearance";
import { dispatch } from "../../runtime/commands";
import {
  appearance,
  beforeMatch,
  characterKits,
  customizerVisible,
  enterArena,
} from "../../state/appearance";
import { setPanel } from "../../state/session";
import { useStore } from "../../state/store";
import { PlayOnline } from "../play-online/PlayOnline";
import { Stage } from "./Customizer";
import { describe } from "./labels";
import "./customizer.css";

/** The home screen before the first round: the saved character, ready to
 * customize, set up keybinds and spells, practice, or play online alone or
 * with a party. A match found while here enters the arena by itself. */
export function StartScreen() {
  const prematch = useStore(beforeMatch, (v) => v);
  const customizing = useStore(customizerVisible, (v) => v);
  const kits = useStore(characterKits, (k) => k);
  const look = useStore(appearance, (a) => a);
  const kit = selectKit(kits, look);
  if (!prematch || customizing || !kit) return null;
  return (
    <div className="customizer">
      <Stage />
      <aside className="start-panel" aria-label="Ready" data-keyboard-owner="true">
        <span className="start-eyebrow">Ready</span>
        <h2>Your fighter</h2>
        <p className="customizer-summary">{describe(kit, look)}</p>
        <div className="start-actions">
          <Button size="3" variant="soft" color="gray" onClick={() => customizerVisible.set(true)}>
            Customize character
          </Button>
          <div className="start-setup">
            <Button variant="surface" color="gray" onClick={() => setPanel("settings")}>
              Keybinds
            </Button>
            <Button variant="surface" color="gray" onClick={() => setPanel("spellbook")}>
              Spells
            </Button>
          </div>
        </div>
        <p className="customizer-note">
          Your look, keybinds and spells are saved on this device. Your look is locked for each
          match once it starts.
        </p>
        <Separator size="4" />
        <PlayOnline />
        <Separator size="4" />
        <Button size="3" onClick={() => enterArena(() => dispatch("reset"))}>
          Practice
        </Button>
      </aside>
    </div>
  );
}
