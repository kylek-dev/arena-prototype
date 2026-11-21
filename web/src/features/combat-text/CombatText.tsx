import type { Snapshot } from "../../bridge/snapshot";
import { type CombatTextProjection, ownOutgoingText } from "../../domain/units/combat-text";
import { error, nameplateScene } from "../../state/session";
import { useStore } from "../../state/store";

const EMPTY: CombatTextProjection[] = [];

// Returning a stable empty array avoids re-rendering on every render-rate
// engine frame while no floating combat numbers are active.
function selectCombatText(scene: Snapshot | null): CombatTextProjection[] | null {
  if (!scene) return null;
  const entries = scene.combatText;
  return entries && entries.length > 0 ? entries : EMPTY;
}

export function CombatText() {
  const entries = useStore(nameplateScene, selectCombatText);
  const failure = useStore(error, (s) => s);
  if (!entries || failure) return null;
  return (
    <div className="floating-combat-text" aria-hidden="true">
      {ownOutgoingText(entries).map((entry) => (
        <span
          key={entry.id}
          className={entry.delta > 0 ? "healing" : "damage"}
          style={{
            left: `${entry.x * 100}%`,
            top: `${entry.y * 100}%`,
            opacity: Math.min(1, (1 - entry.progress) * 3),
          }}
        >
          {entry.delta > 0 ? "+" : ""}
          {Math.abs(entry.delta)}
        </span>
      ))}
    </div>
  );
}
