import { Button } from "@radix-ui/themes";
import { selectKit } from "../../domain/appearance";
import { dispatch } from "../../runtime/commands";
import {
  appearance,
  beforeMatch,
  characterKits,
  customizerVisible,
  enterArena,
} from "../../state/appearance";
import { useStore } from "../../state/store";
import { Stage } from "./Customizer";
import { describe } from "./labels";
import "./customizer.css";

/** Shown before the first round: the saved character, ready to customize or play. */
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
        <span className="start-eyebrow">Local practice</span>
        <h2>Your fighter</h2>
        <p className="customizer-summary">{describe(kit, look)}</p>
        <div className="start-actions">
          <Button size="3" onClick={() => enterArena(() => dispatch("reset"))}>
            Enter the arena
          </Button>
          <Button size="3" variant="soft" color="gray" onClick={() => customizerVisible.set(true)}>
            Customize character
          </Button>
        </div>
        <p className="customizer-note">Your look is saved on this device.</p>
      </aside>
    </div>
  );
}
