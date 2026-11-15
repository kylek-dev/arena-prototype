import { Button } from "@radix-ui/themes";
import { lightingControls, lightingGroups, toneMappingOptions } from "../../domain/lighting";
import { lighting, lightingVisible, resetLighting, updateLighting } from "../../state/lighting";
import { graphics, updateGraphics } from "../../state/graphics";
import { useStore } from "../../state/store";
import { MovableWindow } from "../../ui/MovableWindow";
import "./lighting-tweaks.css";

export function LightingTweaks() {
  const visible = useStore(lightingVisible, (v) => v);
  const values = useStore(lighting, (v) => v);
  const bloom = useStore(graphics, (v) => v.bloom);
  if (!visible) return null;
  return (
    <MovableWindow
      className="lighting-window"
      title="Lighting tweaks (temporary)"
      close={() => lightingVisible.set(false)}
    >
      <div
        className="lighting-tweaks stack"
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.preventDefault();
            e.stopPropagation();
            lightingVisible.set(false);
          }
        }}
      >
        <p>
          Live preview · session only. Drag the title to move this panel. Click the arena to
          move/look around; reopening keeps your tweaks.
        </p>
        <p>
          Start with bloom off. Lower EV100 is brighter. For washed-out colors, reduce ambient light
          or raise saturation/contrast slightly. For dark faces, raise fill light or shadow lift;
          lower highlight gain if bright areas lose detail. Color grading affects the world, not the
          HUD. Local staff halos/sparks stay active independently of bloom.
        </p>
        <label>
          <input
            type="checkbox"
            checked={bloom}
            onChange={(e) => updateGraphics({ bloom: e.target.checked })}
          />
          Enable bloom (device setting)
        </label>
        {!bloom && <p role="status">Bloom is off. Its sliders take effect when you enable it.</p>}
        <label>
          Tone mapping
          <select
            value={values.tonemapping}
            onChange={(e) => updateLighting("tonemapping", Number(e.target.value))}
          >
            {toneMappingOptions.map((name, index) => (
              <option key={name} value={index}>
                {name}
              </option>
            ))}
          </select>
        </label>
        {lightingGroups.map((group) => (
          <fieldset className="stack" key={group.title}>
            <legend>{group.title}</legend>
            {group.controls.map((index) => {
              const control = lightingControls[index];
              return (
                <label className="lighting-tweak" key={control.key}>
                  <span>{control.label}</span>
                  <input
                    type="range"
                    min={control.min}
                    max={control.max}
                    step={control.step}
                    value={values[control.key]}
                    onChange={(e) => updateLighting(control.key, e.target.valueAsNumber)}
                  />
                  <output aria-hidden="true">
                    {values[control.key].toFixed(
                      control.step < 0.01 ? 3 : control.step < 1 ? 2 : 0,
                    )}
                  </output>
                </label>
              );
            })}
          </fieldset>
        ))}
        <Button onClick={resetLighting}>Reset lighting tweaks</Button>
        <details>
          <summary>Current values · select and copy before reloading</summary>
          <textarea
            aria-label="Current lighting values"
            readOnly
            value={JSON.stringify(values, null, 2)}
            rows={12}
          />
        </details>
      </div>
    </MovableWindow>
  );
}
