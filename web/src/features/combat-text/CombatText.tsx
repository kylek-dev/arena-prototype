import { ownOutgoingText } from "../../domain/units/combat-text";
import { error, nameplateScene } from "../../state/session";
import { useStore } from "../../state/store";

export function CombatText() {
  const scene = useStore(nameplateScene, (s) => s);
  const failure = useStore(error, (s) => s);
  if (!scene || failure) return null;
  return (
    <div className="floating-combat-text" aria-hidden="true">
      {ownOutgoingText(scene.combatText ?? []).map((entry) => (
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
