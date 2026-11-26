import { defaultLighting, lightingValues } from "../domain/lighting";
import { createStore } from "./store";

// The arena's lighting, read by the renderer each frame: the authored
// preset (assets/lighting.json). The temporary tweaks window was removed
// (playtest inbox, 2026-11-25); the preset is edited as a file.
export const lighting = createStore({ ...defaultLighting });
export const readLighting = () => lightingValues(lighting.get());
