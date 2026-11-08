import { defaultGraphics, graphicsValues, parseGraphics } from "../domain/graphics";
import { createStore } from "./store";

export const GRAPHICS_KEY = "arena.graphics.v1";

export function loadGraphics(storage: Pick<Storage, "getItem">) {
  try {
    return (
      parseGraphics(JSON.parse(storage.getItem(GRAPHICS_KEY) ?? "null")) ?? {
        ...defaultGraphics,
      }
    );
  } catch {
    return { ...defaultGraphics };
  }
}

function initialGraphics() {
  try {
    return loadGraphics(localStorage);
  } catch {
    return { ...defaultGraphics };
  }
}

export const graphics = createStore(initialGraphics());
export const graphicsSaveError = createStore("");
export const readGraphics = () => graphicsValues(graphics.get());

export function updateGraphics(
  patch: Partial<Pick<ReturnType<typeof graphics.get>, "antialiasing" | "modelDetail" | "bloom">>,
) {
  const next = parseGraphics({ ...graphics.get(), ...patch });
  if (!next) return;
  graphics.set(next);
  try {
    localStorage.setItem(GRAPHICS_KEY, JSON.stringify(next));
    graphicsSaveError.set("");
  } catch {
    graphicsSaveError.set("Graphics settings apply now but could not be saved on this device.");
  }
}
