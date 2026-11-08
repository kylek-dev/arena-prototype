// Temporary session-only art controls. Order is the presentation callback ABI.
export const lightingControls = [
  { key: "backgroundR", label: "Background red", min: 0, max: 1, step: 0.01, initial: 0.18 },
  { key: "backgroundG", label: "Background green", min: 0, max: 1, step: 0.01, initial: 0.21 },
  { key: "backgroundB", label: "Background blue", min: 0, max: 1, step: 0.01, initial: 0.25 },
  { key: "ambient", label: "Ambient light", min: 0, max: 3000, step: 10, initial: 500 },
  { key: "keyLight", label: "Main light (lux)", min: 0, max: 20000, step: 100, initial: 6500 },
  { key: "fillLight", label: "Fill light (lux)", min: 0, max: 20000, step: 100, initial: 3200 },
  { key: "exposure", label: "Exposure (EV100)", min: 4, max: 14, step: 0.1, initial: 10 },
  { key: "bloomIntensity", label: "Bloom intensity", min: 0, max: 1, step: 0.01, initial: 0.22 },
  { key: "bloomThreshold", label: "Bloom threshold", min: 0, max: 5, step: 0.05, initial: 0.8 },
  { key: "bloomSpread", label: "Bloom wide halo", min: 0, max: 1, step: 0.01, initial: 0.4 },
  { key: "saturation", label: "Color saturation", min: 0, max: 2, step: 0.01, initial: 1 },
  { key: "contrast", label: "Contrast", min: 0.5, max: 1.5, step: 0.01, initial: 1 },
  { key: "gamma", label: "Gamma", min: 0.5, max: 1.5, step: 0.01, initial: 1 },
  { key: "shadows", label: "Shadow lift", min: -0.1, max: 0.2, step: 0.005, initial: 0 },
  { key: "midtones", label: "Midtone gain", min: 0.25, max: 2, step: 0.01, initial: 1 },
  { key: "highlights", label: "Highlight gain", min: 0.25, max: 2, step: 0.01, initial: 1 },
  {
    key: "temperature",
    label: "Warm / cool balance",
    min: -0.05,
    max: 0.05,
    step: 0.001,
    initial: 0,
  },
  { key: "tint", label: "Green / magenta tint", min: -0.05, max: 0.05, step: 0.001, initial: 0 },
  { key: "keyR", label: "Main light red", min: 0, max: 1, step: 0.01, initial: 1 },
  { key: "keyG", label: "Main light green", min: 0, max: 1, step: 0.01, initial: 0.95 },
  { key: "keyB", label: "Main light blue", min: 0, max: 1, step: 0.01, initial: 0.88 },
  { key: "fillR", label: "Fill light red", min: 0, max: 1, step: 0.01, initial: 0.88 },
  { key: "fillG", label: "Fill light green", min: 0, max: 1, step: 0.01, initial: 0.93 },
  { key: "fillB", label: "Fill light blue", min: 0, max: 1, step: 0.01, initial: 1 },
  {
    key: "keyPitch",
    label: "Main light elevation (degrees)",
    min: 5,
    max: 90,
    step: 1,
    initial: 51.5662,
  },
  {
    key: "keyYaw",
    label: "Main light rotation (degrees)",
    min: -180,
    max: 180,
    step: 1,
    initial: -31.5127,
  },
  {
    key: "fillPitch",
    label: "Fill light elevation (degrees)",
    min: 5,
    max: 90,
    step: 1,
    initial: 25.7831,
  },
  {
    key: "fillYaw",
    label: "Fill light rotation (degrees)",
    min: -180,
    max: 180,
    step: 1,
    initial: 131.7803,
  },
  { key: "tonemapping", label: "Tone mapping", min: 0, max: 2, step: 1, initial: 0 },
] as const;
export const toneMappingOptions = ["Reinhard (current)", "Reinhard luminance", "ACES filmic"];
// Group independently of callback order: existing positions must never move.
export const lightingGroups = [
  { title: "Brightness & color", controls: [6, 10, 11, 12, 13, 14, 15, 16, 17] },
  { title: "Light strength", controls: [3, 4, 5] },
  { title: "Light colors & direction", controls: [18, 19, 20, 21, 22, 23, 24, 25, 26, 27] },
  { title: "Background", controls: [0, 1, 2] },
  { title: "Bloom", controls: [7, 8, 9] },
];
export type LightingKey = (typeof lightingControls)[number]["key"];
export type LightingValues = Record<LightingKey, number>;
export const defaultLighting = Object.fromEntries(
  lightingControls.map((c) => [c.key, c.initial]),
) as LightingValues;
export function patchLighting(
  current: LightingValues,
  key: LightingKey,
  value: number,
): LightingValues {
  const control = lightingControls.find((c) => c.key === key);
  if (!control || !Number.isFinite(value)) return current;
  const bounded = Math.max(control.min, Math.min(control.max, value));
  return { ...current, [key]: key === "tonemapping" ? Math.round(bounded) : bounded };
}
export const lightingValues = (value: LightingValues): number[] =>
  lightingControls.map((c) => value[c.key]);
