import { bind, conflicts, type Chord } from "../domain/bindings/model";
import { config, updateConfig } from "./config";
import { createStore } from "./store";
import { interaction } from "./session";
export const captureMessage = createStore("");
export const pendingBinding = createStore<{
  action: string;
  entry: 0 | 1;
  value: Chord;
  conflicts: ReturnType<typeof conflicts>;
} | null>(null);
export function acceptBinding(action: string, entry: 0 | 1, value: Chord | null, replace = false) {
  const collisions = value ? conflicts(config().bindings, value, action, entry) : [];
  if (collisions.length && !replace && value) {
    pendingBinding.set({ action, entry, value, conflicts: collisions });
    return;
  }
  updateConfig((c) => ({ ...c, bindings: bind(c.bindings, action, entry, value, replace) }));
  pendingBinding.set(null);
  captureMessage.set("Binding updated.");
  interaction.set({ ...interaction.get(), capture: null });
}
