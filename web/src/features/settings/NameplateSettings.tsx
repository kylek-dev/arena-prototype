import { useConfigSelector } from "../../state/config";
import { setNameplates } from "../../runtime/nameplate-feedback";

export function NameplateSettings() {
  const preferences = useConfigSelector((c) => c.nameplates);
  return (
    <div className="stack">
      <p>
        Floating health bars for other characters. Your own character never has a nameplate. Saved
        with this interface profile.
      </p>
      {(["enemy", "friendly"] as const).map((side) => (
        <label key={side}>
          <input
            type="checkbox"
            checked={preferences[side]}
            onChange={(e) => {
              const checked = e.target.checked;
              setNameplates(side, checked);
            }}
          />
          {side === "enemy" ? "Enemy nameplates" : "Friendly nameplates"}
        </label>
      ))}
      <p>
        Left-click a bar to target; right-click to focus. Default toggles: V for enemies, Shift+V
        for friends. Remap in Bindings. The practice scene currently has no teammates.
      </p>
    </div>
  );
}
