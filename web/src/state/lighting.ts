import {
  defaultLighting,
  lightingValues,
  patchLighting,
  type LightingKey,
} from "../domain/lighting";
import { createStore } from "./store";

export const lightingVisible = createStore(false);
export const lighting = createStore({ ...defaultLighting });
export const readLighting = () => lightingValues(lighting.get());
export const updateLighting = (key: LightingKey, value: number) =>
  lighting.set(patchLighting(lighting.get(), key, value));
export const resetLighting = () => lighting.set({ ...defaultLighting });
