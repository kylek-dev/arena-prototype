import { defaultGraphics, type GraphicsPreferences } from "../../domain/graphics";
import { graphics, graphicsSaveError, updateGraphics } from "../../state/graphics";
import { useStore } from "../../state/store";
import { Button } from "@radix-ui/themes";

export function GraphicsSettings() {
  const preferences = useStore(graphics, (v) => v);
  const error = useStore(graphicsSaveError, (v) => v);
  return (
    <div className="stack">
      <p>Applies immediately. Saved on this device, separately from interface profiles.</p>
      <label>
        Anti-aliasing
        <select
          value={preferences.antialiasing}
          onChange={(e) =>
            updateGraphics({ antialiasing: e.target.value as GraphicsPreferences["antialiasing"] })
          }
        >
          <option value="off">Off · lower GPU cost</option>
          <option value="msaa4">4× MSAA · smoother edges (recommended)</option>
        </select>
      </label>
      <p>Smooths floor lines and object outlines. Turn off for better performance.</p>
      <label>
        Model detail
        <select
          value={preferences.modelDetail}
          onChange={(e) =>
            updateGraphics({ modelDetail: e.target.value as GraphicsPreferences["modelDetail"] })
          }
        >
          <option value="low">Low</option>
          <option value="high">High · rounder procedural characters</option>
        </select>
      </label>
      <p>
        Model detail affects procedural characters only, not imported models. Pillars remain square.
        Rendering stays at native resolution; interface text is unaffected.
      </p>
      <Button onClick={() => updateGraphics(defaultGraphics)}>Reset graphics</Button>
      {error && <p role="status">{error}</p>}
    </div>
  );
}
