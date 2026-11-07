export interface GraphicsPreferences {
  version: 1;
  antialiasing: "off" | "msaa4";
  modelDetail: "low" | "high";
}

export const defaultGraphics: GraphicsPreferences = {
  version: 1,
  antialiasing: "msaa4",
  modelDetail: "high",
};

export function parseGraphics(value: unknown): GraphicsPreferences | null {
  if (!value || typeof value !== "object") return null;
  const v = value as Record<string, unknown>;
  if (
    v.version !== 1 ||
    (v.antialiasing !== "off" && v.antialiasing !== "msaa4") ||
    (v.modelDetail !== "low" && v.modelDetail !== "high")
  )
    return null;
  return { version: 1, antialiasing: v.antialiasing, modelDetail: v.modelDetail };
}

// Separate presentation callback, never part of the combat command array.
export function graphicsValues(value: GraphicsPreferences): number[] {
  return [value.antialiasing === "msaa4" ? 4 : 1, value.modelDetail === "high" ? 1 : 0];
}
