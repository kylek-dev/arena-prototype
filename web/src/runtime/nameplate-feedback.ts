import { config, updateConfig } from "../state/config";
import { createStore } from "../state/store";

export const nameplateNotice = createStore<{ message: string } | null>(null);

export function setNameplates(side: "enemy" | "friendly", enabled: boolean) {
  if (config().nameplates[side] === enabled) return;
  updateConfig((c) => ({ ...c, nameplates: { ...c.nameplates, [side]: enabled } }));
  nameplateNotice.set({
    message: `${side === "enemy" ? "Enemy" : "Friendly"} nameplates ${enabled ? "enabled" : "disabled"}`,
  });
}
