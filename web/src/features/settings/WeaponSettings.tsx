import { weaponColors, type WeaponPreferences } from "../../domain/weapons";
import { weapons, weaponsSaveError, updateWeapons } from "../../state/weapons";
import { useStore } from "../../state/store";

export function WeaponSettings() {
  const preferences = useStore(weapons, (v) => v);
  const error = useStore(weaponsSaveError, (v) => v);
  return (
    <div className="stack">
      <p>
        Cosmetic back weapons only. Applies immediately and saves on this device. Combat stats and
        abilities stay unchanged.
      </p>
      <label>
        Weapon
        <select
          value={preferences.weapon}
          onChange={(e) => updateWeapons({ weapon: e.target.value as WeaponPreferences["weapon"] })}
        >
          <option value="glaives">Verdant Eclipse · paired glaives</option>
          <option value="staff">Aetherthorn · crystal staff</option>
        </select>
      </label>
      <label>
        Energy color
        <select
          value={preferences.color}
          onChange={(e) => updateWeapons({ color: e.target.value as WeaponPreferences["color"] })}
        >
          {weaponColors.map((color) => (
            <option key={color} value={color}>
              {color[0].toUpperCase() + color.slice(1)}
            </option>
          ))}
        </select>
      </label>
      <p>
        Both weapons support purple, yellow, red, blue, green and cyan. Metal and grip finishes are
        preserved. A procedural weapon appears while the model loads or if loading fails.
      </p>
      {error && <p role="status">{error}</p>}
    </div>
  );
}
