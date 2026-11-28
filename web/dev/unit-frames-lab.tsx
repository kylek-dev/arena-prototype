/**
 * Dev-only style lab for the unit frames: `npm run dev`, then open
 * http://127.0.0.1:8000/dev/unit-frames.html
 *
 * It renders the real `UnitFrame` component with the game's stylesheet,
 * then layers the CSS generated from the controls on top. Edits to
 * `unit-frames.css` also hot-reload here. "Copy CSS" gives rules to paste
 * over the matching ones in `web/src/features/unit-frames/unit-frames.css`.
 * Not part of the production build (vite.config.ts lists its pages).
 */
import { Theme } from "@radix-ui/themes";
import { useEffect, useMemo, useState, type ReactNode } from "react";
import { createRoot } from "react-dom/client";
import { UnitFrame } from "../src/features/unit-frames/UnitFrame";
import type { Entity } from "../src/domain/units/selection";
import "@radix-ui/themes/styles.css";
import "../src/styles/global.css";

type Control =
  | { key: string; label: string; kind: "range"; min: number; max: number; step?: number; unit?: string }
  | { key: string; label: string; kind: "color" }
  | { key: string; label: string; kind: "check" }
  | { key: string; label: string; kind: "select"; options: string[] };

const TEXTURES = [
  "none",
  "stone-granite",
  "stone-wall",
  "stone-bricks",
  "stone-floor",
  "metal-grate",
  "metal-panels",
  "ground-sand",
];
const FONTS = [
  "inherit",
  "Inter, system-ui, sans-serif",
  "'Segoe UI', sans-serif",
  "Arial, sans-serif",
  "Verdana, sans-serif",
  "Tahoma, sans-serif",
  "'Trebuchet MS', sans-serif",
  "Bahnschrift, sans-serif",
];
const BLENDS = ["overlay", "multiply", "soft-light", "screen", "luminosity", "normal"];

// Defaults match the current unit-frames.css.
const DEFAULTS: Record<string, number | string | boolean> = {
  width: 236,
  padding: 6,
  frameColor: "#111a23",
  frameOpacity: 0,
  borderWidth: 1,
  borderColor: "#3b4452",
  borderOpacity: 0,
  radius: 0,
  shadowBlur: 12,
  shadowY: 4,
  shadowOpacity: 0,
  frameTexture: "none",
  frameTextureOpacity: 40,
  frameTextureBlend: "overlay",
  listGap: 8,
  font: "inherit",
  nameSize: 14,
  nameWeight: 600,
  nameColor: "#e9eef3",
  nameGap: 2,
  healthHeight: 33,
  resourceHeight: 17,
  barGap: 2,
  barRadius: 3,
  trackColor: "#292929",
  trackFollowsFrame: false,
  trackOpacity: 77,
  trackBorderWidth: 1,
  trackBorderColor: "#0a1118",
  healthColor: "#3fa36b",
  resourceColor: "#438bdd",
  gradientLight: 10,
  gradientMid: 45,
  gradientDark: 35,
  gloss: 15,
  fillTexture: "ground-sand",
  fillTextureOpacity: 100,
  fillTextureBlend: "overlay",
  valueSize: 10,
  valueWeight: 600,
  valueColor: "#e9eef3",
  valueShadow: 100,
  castHeight: 21,
  castColor: "#c3af32",
  castBorderColor: "#0a1118",
};

const GROUPS: { title: string; controls: Control[] }[] = [
  {
    title: "Frame",
    controls: [
      { key: "width", label: "Width", kind: "range", min: 140, max: 400, unit: "px" },
      { key: "padding", label: "Padding", kind: "range", min: 0, max: 20, unit: "px" },
      { key: "frameColor", label: "Background", kind: "color" },
      {
        key: "frameOpacity",
        label: "Background opacity (the per-frame layout setting)",
        kind: "range",
        min: 0,
        max: 100,
        unit: "%",
      },
      { key: "borderWidth", label: "Border width", kind: "range", min: 0, max: 6, unit: "px" },
      { key: "borderColor", label: "Border colour", kind: "color" },
      { key: "borderOpacity", label: "Border opacity", kind: "range", min: 0, max: 100, unit: "%" },
      { key: "radius", label: "Corner radius", kind: "range", min: 0, max: 16, unit: "px" },
      { key: "shadowBlur", label: "Shadow blur", kind: "range", min: 0, max: 40, unit: "px" },
      { key: "shadowY", label: "Shadow offset", kind: "range", min: 0, max: 16, unit: "px" },
      { key: "shadowOpacity", label: "Shadow opacity", kind: "range", min: 0, max: 100, unit: "%" },
      { key: "frameTexture", label: "Frame texture", kind: "select", options: TEXTURES },
      { key: "frameTextureOpacity", label: "Frame texture opacity", kind: "range", min: 0, max: 100, unit: "%" },
      { key: "frameTextureBlend", label: "Frame texture blend", kind: "select", options: BLENDS },
      { key: "listGap", label: "Gap between team/opponent frames", kind: "range", min: 0, max: 24, unit: "px" },
    ],
  },
  {
    title: "Name",
    controls: [
      { key: "font", label: "Font", kind: "select", options: FONTS },
      { key: "nameSize", label: "Size", kind: "range", min: 9, max: 22, unit: "px" },
      { key: "nameWeight", label: "Weight", kind: "range", min: 300, max: 900, step: 100 },
      { key: "nameColor", label: "Colour", kind: "color" },
      { key: "nameGap", label: "Space below", kind: "range", min: 0, max: 12, unit: "px" },
    ],
  },
  {
    title: "Health and resource bars",
    controls: [
      { key: "healthHeight", label: "Health height", kind: "range", min: 4, max: 40, unit: "px" },
      { key: "resourceHeight", label: "Resource height", kind: "range", min: 2, max: 40, unit: "px" },
      { key: "barGap", label: "Gap between bars", kind: "range", min: 0, max: 12, unit: "px" },
      { key: "barRadius", label: "Corner radius", kind: "range", min: 0, max: 12, unit: "px" },
      { key: "trackColor", label: "Empty track colour", kind: "color" },
      { key: "trackFollowsFrame", label: "Track opacity follows frame background", kind: "check" },
      { key: "trackOpacity", label: "Track opacity (when not following)", kind: "range", min: 0, max: 100, unit: "%" },
      { key: "trackBorderWidth", label: "Track border", kind: "range", min: 0, max: 4, unit: "px" },
      { key: "trackBorderColor", label: "Track border colour", kind: "color" },
      { key: "healthColor", label: "Health colour", kind: "color" },
      { key: "resourceColor", label: "Resource colour", kind: "color" },
      { key: "gradientLight", label: "Gradient: lighten top", kind: "range", min: 0, max: 60, unit: "%" },
      { key: "gradientMid", label: "Gradient: midpoint", kind: "range", min: 0, max: 100, unit: "%" },
      { key: "gradientDark", label: "Gradient: darken bottom", kind: "range", min: 0, max: 60, unit: "%" },
      { key: "gloss", label: "Top highlight line", kind: "range", min: 0, max: 80, unit: "%" },
      { key: "fillTexture", label: "Fill texture", kind: "select", options: TEXTURES },
      { key: "fillTextureOpacity", label: "Fill texture opacity", kind: "range", min: 0, max: 100, unit: "%" },
      { key: "fillTextureBlend", label: "Fill texture blend", kind: "select", options: BLENDS },
    ],
  },
  {
    title: "Bar text",
    controls: [
      { key: "valueSize", label: "Size", kind: "range", min: 8, max: 20, unit: "px" },
      { key: "valueWeight", label: "Weight", kind: "range", min: 300, max: 900, step: 100 },
      { key: "valueColor", label: "Colour", kind: "color" },
      { key: "valueShadow", label: "Shadow", kind: "range", min: 0, max: 100, unit: "%" },
    ],
  },
  {
    title: "Cast bar (in frames)",
    controls: [
      { key: "castHeight", label: "Height", kind: "range", min: 8, max: 32, unit: "px" },
      { key: "castColor", label: "Fill colour", kind: "color" },
      { key: "castBorderColor", label: "Border colour", kind: "color" },
    ],
  },
];

type Values = typeof DEFAULTS;
const STORAGE = "pitborn.unit-frame-lab";

const texture = (name: unknown) => `url("/assets/textures/${name}.png")`;
const mix = (color: unknown, pct: unknown, to = "transparent") =>
  `color-mix(in srgb, ${color} ${pct}%, ${to})`;

/** The rules for unit-frames.css. Here they load after the game's own
 * stylesheet (and the extra CSS after them), so they win by order. */
function stylesheet(v: Values, scope = "") {
  const s = (selectors: string) =>
    selectors
      .split(",")
      .map((sel) => `${scope}${sel.trim()}`)
      .join(",\n");
  const track = v.trackFollowsFrame
    ? `color-mix(in srgb, ${v.trackColor} var(--hud-background, ${v.frameOpacity}%), transparent)`
    : mix(v.trackColor, v.trackOpacity);
  const gradient = (color: string) =>
    `linear-gradient(\n    180deg,\n    ${mix(color, 100 - Number(v.gradientLight), "var(--pal-night-12)")} 0%,\n    ${color} ${v.gradientMid}%,\n    ${mix(color, 100 - Number(v.gradientDark), "var(--pal-night-1)")} 100%\n  )`;
  const rules: string[] = [];
  rules.push(`${s(".unit-frame")} {
  position: relative;
  isolation: isolate;
  width: ${v.width}px;
  padding: ${v.padding}px;
  font-family: ${v.font};
  background: color-mix(in srgb, ${v.frameColor} var(--hud-background, ${v.frameOpacity}%), transparent);
  border: ${v.borderWidth}px solid ${mix(v.borderColor, v.borderOpacity)};
  border-radius: ${v.radius}px;
  box-shadow: ${Number(v.shadowOpacity) > 0 ? `0 ${v.shadowY}px ${v.shadowBlur}px ${mix("var(--pal-night-1)", v.shadowOpacity)}` : "none"};
}`);
  if (v.frameTexture !== "none")
    rules.push(`${s(".unit-frame::before")} {
  content: "";
  position: absolute;
  inset: 0;
  z-index: -1;
  border-radius: inherit;
  background: ${texture(v.frameTexture)} center / 256px;
  opacity: ${Number(v.frameTextureOpacity) / 100};
  mix-blend-mode: ${v.frameTextureBlend};
  pointer-events: none;
}`);
  rules.push(`${s(".opponent-list")} {
  gap: ${v.listGap}px;
}`);
  rules.push(`${s(".unit-heading")} {
  margin: 0 0 ${v.nameGap}px 2px;
}`);
  rules.push(`${s(".unit-heading strong")} {
  font-size: ${v.nameSize}px;
  font-weight: ${v.nameWeight};
  color: ${v.nameColor};
}`);
  rules.push(`${s(".health-track, .resource-track")} {
  height: ${v.healthHeight}px;
  background: ${track};
  border: ${v.trackBorderWidth}px solid ${Number(v.trackBorderWidth) > 0 ? v.trackBorderColor : "transparent"};
  border-radius: ${v.barRadius}px;
  margin-bottom: ${v.barGap}px;
}`);
  rules.push(`${s(".resource-track")} {
  height: ${v.resourceHeight}px;
}`);
  rules.push(`${s(".health-fill, .resource-fill")} {
  position: relative;
  height: 100%;
  background: ${gradient("var(--bar-color)")};
  box-shadow: ${Number(v.gloss) > 0 ? `inset 0 1px 0 ${mix("var(--pal-night-12)", v.gloss)}` : "none"};
}`);
  if (v.fillTexture !== "none")
    rules.push(`${s(".health-fill::after, .resource-fill::after")} {
  content: "";
  position: absolute;
  inset: 0;
  background: ${texture(v.fillTexture)} center / 128px;
  opacity: ${Number(v.fillTextureOpacity) / 100};
  mix-blend-mode: ${v.fillTextureBlend};
  pointer-events: none;
}`);
  rules.push(`${s(".health-fill")} {
  --bar-color: ${v.healthColor};
}`);
  rules.push(`${s(".resource-fill")} {
  --bar-color: ${v.resourceColor};
}`);
  rules.push(`${s(".bar-value")} {
  z-index: 1;
  color: ${v.valueColor};
  font-size: ${v.valueSize}px;
  font-weight: ${v.valueWeight};
  line-height: 1;
  text-shadow: ${Number(v.valueShadow) > 0 ? `0 1px 2px ${mix("var(--pal-night-1)", v.valueShadow)}` : "none"};
}`);
  rules.push(`${s(".cast-track")} {
  height: ${v.castHeight}px;
  border-color: ${v.castBorderColor};
  border-radius: ${v.barRadius}px;
}`);
  rules.push(`${s(".cast-fill")} {
  background: ${gradient(String(v.castColor))};
}`);
  return rules.join("\n");
}

function load(): Values {
  try {
    return { ...DEFAULTS, ...JSON.parse(localStorage.getItem(STORAGE) ?? "{}").values };
  } catch {
    return { ...DEFAULTS };
  }
}

const BACKDROPS: Record<string, string> = {
  grass: `${texture("ground-grass")} center / 512px`,
  stone: `${texture("stone-floor")} center / 512px`,
  sand: `${texture("ground-sand")} center / 512px`,
  snow: `${texture("ground-snow")} center / 512px`,
  sky: `${texture("sky")} center / cover`,
  dark: "#0a1118",
  grey: "#6e7a8a",
  checker: "repeating-conic-gradient(#3b4452 0 25%, #1e2834 0 50%) 0 0 / 32px 32px",
};

function Lab() {
  const [values, setValues] = useState<Values>(load);
  const [extra, setExtra] = useState<string>(() => {
    try {
      return JSON.parse(localStorage.getItem(STORAGE) ?? "{}").extra ?? "";
    } catch {
      return "";
    }
  });
  const [backdrop, setBackdrop] = useState("grass");
  const [screenshot, setScreenshot] = useState<string | null>(null);
  const [health, setHealth] = useState(75);
  const [resource, setResource] = useState(60);
  const [zoom, setZoom] = useState(1);
  const [castLoop, setCastLoop] = useState(true);
  const [cycle, setCycle] = useState(0);
  const [copied, setCopied] = useState("");

  useEffect(() => {
    localStorage.setItem(STORAGE, JSON.stringify({ values, extra }));
  }, [values, extra]);
  useEffect(() => {
    if (!castLoop) return;
    const timer = setInterval(() => setCycle((c) => c + 1), 2600);
    return () => clearInterval(timer);
  }, [castLoop]);

  const live = useMemo(() => stylesheet(values), [values]);
  const output = useMemo(
    () =>
      `/* Unit frames, from the style lab (web/dev/unit-frames.html).\n   Background opacity ${values.frameOpacity}%: the per-frame default is\n   FRAME_BACKGROUND in web/src/domain/layout/model.ts. Picked colours are\n   hex: the game uses palette swatches only (docs/palette.json). */\n${stylesheet(values)}${extra.trim() ? `\n/* Extra */\n${extra.trim()}` : ""}\n`,
    [values, extra],
  );

  const set = (key: string, value: number | string | boolean) =>
    setValues((v) => ({ ...v, [key]: value }));

  const unit = (id: string, name: string, more: Partial<Entity> = {}): Entity => ({
    id,
    name,
    health: Math.round(health * 4),
    maxHealth: 400,
    resource: resource,
    maxResource: 100,
    ...more,
  });
  const cast = castLoop ? { spell: "rift_bolt" as const, remainingTicks: 60, totalTicks: 75 } : null;

  async function copy() {
    try {
      await navigator.clipboard.writeText(output);
      setCopied("Copied");
    } catch {
      setCopied("Select the text below and copy it");
    }
    setTimeout(() => setCopied(""), 2000);
  }

  return (
    <Theme appearance="dark" accentColor="indigo" grayColor="slate" radius="medium" className="lab-theme">
      <style>{LAB_CSS}</style>
      <style>{live}</style>
      <style>{extra}</style>
      <div className="lab">
        <div
          className="lab-stage"
          style={{
            background: screenshot ? `url("${screenshot}") center / cover` : BACKDROPS[backdrop],
            ["--hud-background" as string]: `${values.frameOpacity}%`,
          }}
        >
          <div className="lab-zoom" style={{ transform: `scale(${zoom})` }}>
            <Spot x={24} y={24} label="Player">
              <UnitFrame role="player" unit={unit(`player-${cycle}`, "Kael", { cast })} />
            </Spot>
            <Spot x={300} y={24} label="Target (admin)">
              <UnitFrame
                role="target"
                unit={unit("target", "Verdant#1024", { admin: true, health: 180 })}
              />
            </Spot>
            <Spot x={24} y={200} label="Team (one disconnected)">
              <div className="opponent-list">
                <UnitFrame role="teammate" unit={unit("t1", "Teammate One")} />
                <UnitFrame role="teammate" unit={unit("t2", "Teammate Two", { disconnected: true })} />
              </div>
            </Spot>
            <Spot x={300} y={200} label="Focus (locked out)">
              <UnitFrame
                role="focus"
                unit={unit(`focus-${cycle}`, "Enemy Caster", { lockoutRemaining: 30, cast })}
              />
            </Spot>
            <Spot x={576} y={24} label="Opponents">
              <div className="opponent-list">
                <UnitFrame role="opponent" unit={unit("o1", "Enemy Bot 1", { health: 400 })} />
                <UnitFrame role="opponent" unit={unit("o2", "Enemy Bot 2")} />
                <UnitFrame role="opponent" unit={unit("o3", "Enemy Bot 3", { health: 40, resource: 10 })} />
              </div>
            </Spot>
            <Spot x={576} y={340} label="Empty target">
              <UnitFrame role="target" unit={null} />
            </Spot>
          </div>
        </div>
        <aside className="lab-panel">
          <h1>Unit frame lab</h1>
          <p className="lab-hint">
            The real frames with the game stylesheet. Editing <code>unit-frames.css</code> also
            hot-reloads here. Settings are saved in this browser.
          </p>
          <div className="lab-actions">
            <button type="button" onClick={copy}>
              Copy CSS
            </button>
            <button
              type="button"
              onClick={() => {
                setValues({ ...DEFAULTS });
                setExtra("");
              }}
            >
              Reset to current
            </button>
            <span>{copied}</span>
          </div>
          <fieldset>
            <legend>Preview</legend>
            <label>
              Backdrop
              <select value={backdrop} onChange={(e) => setBackdrop(e.target.value)}>
                {Object.keys(BACKDROPS).map((name) => (
                  <option key={name}>{name}</option>
                ))}
              </select>
            </label>
            <label>
              Game screenshot as backdrop
              <input
                type="file"
                accept="image/*"
                onChange={(e) => {
                  const file = e.target.files?.[0];
                  setScreenshot(file ? URL.createObjectURL(file) : null);
                }}
              />
            </label>
            {screenshot && (
              <button type="button" onClick={() => setScreenshot(null)}>
                Remove screenshot
              </button>
            )}
            <Range label="Zoom" value={zoom} min={0.6} max={2} step={0.05} text={`${Math.round(zoom * 100)}%`} onChange={setZoom} />
            <Range label="Sample health" value={health} min={0} max={100} text={`${health}%`} onChange={setHealth} />
            <Range label="Sample resource" value={resource} min={0} max={100} text={`${resource}%`} onChange={setResource} />
            <label className="lab-check">
              <input type="checkbox" checked={castLoop} onChange={(e) => setCastLoop(e.target.checked)} />
              Show looping casts
            </label>
          </fieldset>
          {GROUPS.map((group) => (
            <fieldset key={group.title}>
              <legend>{group.title}</legend>
              {group.controls.map((c) => (
                <Field key={c.key} control={c} value={values[c.key]} set={set} />
              ))}
            </fieldset>
          ))}
          <fieldset>
            <legend>Extra CSS (applied live, included in the copy)</legend>
            <textarea
              value={extra}
              spellCheck={false}
              rows={6}
              placeholder={".unit-heading strong {\n  text-transform: uppercase;\n}"}
              onChange={(e) => setExtra(e.target.value)}
            />
          </fieldset>
          <fieldset>
            <legend>Generated CSS</legend>
            <textarea readOnly value={output} rows={14} spellCheck={false} onFocus={(e) => e.target.select()} />
          </fieldset>
        </aside>
      </div>
    </Theme>
  );
}

function Spot({ x, y, label, children }: { x: number; y: number; label: string; children: ReactNode }) {
  return (
    <div className="lab-spot" style={{ left: x, top: y }}>
      <span className="lab-spot-label">{label}</span>
      {children}
    </div>
  );
}

function Range(props: {
  label: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  text: string;
  onChange: (n: number) => void;
}) {
  return (
    <label>
      <span className="lab-row">
        {props.label}
        <b>{props.text}</b>
      </span>
      <input
        type="range"
        min={props.min}
        max={props.max}
        step={props.step ?? 1}
        value={props.value}
        onChange={(e) => props.onChange(+e.target.value)}
      />
    </label>
  );
}

function Field({
  control: c,
  value,
  set,
}: {
  control: Control;
  value: number | string | boolean;
  set: (key: string, value: number | string | boolean) => void;
}) {
  if (c.kind === "range")
    return (
      <Range
        label={c.label}
        value={Number(value)}
        min={c.min}
        max={c.max}
        step={c.step}
        text={`${value}${c.unit ?? ""}`}
        onChange={(n) => set(c.key, n)}
      />
    );
  if (c.kind === "color")
    return (
      <label className="lab-row">
        {c.label}
        <span className="lab-color">
          <code>{String(value)}</code>
          <input type="color" value={String(value)} onChange={(e) => set(c.key, e.target.value)} />
        </span>
      </label>
    );
  if (c.kind === "check")
    return (
      <label className="lab-check">
        <input type="checkbox" checked={!!value} onChange={(e) => set(c.key, e.target.checked)} />
        {c.label}
      </label>
    );
  return (
    <label>
      {c.label}
      <select value={String(value)} onChange={(e) => set(c.key, e.target.value)}>
        {c.options.map((o) => (
          <option key={o}>{o}</option>
        ))}
      </select>
    </label>
  );
}

const LAB_CSS = `
body { overflow: hidden; }
.lab-theme { min-height: 100vh; }
.lab { display: grid; grid-template-columns: 1fr 380px; height: 100vh; }
.lab-stage { position: relative; overflow: auto; }
.lab-zoom { position: relative; width: 880px; height: 560px; transform-origin: top left; }
.lab-spot { position: absolute; }
.lab-spot-label {
  display: block; margin-bottom: 4px; font-size: 11px; color: #e9eef3;
  text-shadow: 0 1px 2px #000; opacity: 0.8;
}
.lab-panel {
  overflow-y: auto; padding: 14px 16px 40px; background: #0a1118;
  border-left: 1px solid #2e3541; font-size: 13px; color: #e9eef3;
}
.lab-panel h1 { font-size: 18px; margin: 0 0 6px; }
.lab-hint { color: #aab3bf; margin: 0 0 10px; }
.lab-panel fieldset {
  display: grid; gap: 8px; margin: 0 0 12px; padding: 10px;
  border: 1px solid #2e3541; border-radius: 6px;
}
.lab-panel legend { padding: 0 4px; font-weight: 600; color: #aab3bf; }
.lab-panel label { display: grid; gap: 3px; }
.lab-panel input[type="range"] { width: 100%; }
.lab-row { display: flex; justify-content: space-between; align-items: center; gap: 8px; }
.lab-row b { font-weight: 600; font-variant-numeric: tabular-nums; color: #aab3bf; }
.lab-check { display: flex !important; align-items: center; gap: 6px; }
.lab-color { display: flex; align-items: center; gap: 6px; }
.lab-color input { width: 40px; height: 26px; padding: 0; border: 0; background: none; }
.lab-actions { display: flex; align-items: center; gap: 8px; margin-bottom: 12px; }
.lab-actions span { color: #3fa36b; }
.lab-panel textarea { width: 100%; font: 12px/1.4 ui-monospace, monospace; resize: vertical; }
`;

createRoot(document.getElementById("lab")!).render(<Lab />);
