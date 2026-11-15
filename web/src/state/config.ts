import { createStore, useStore } from "./store";
import { initial, type Config, type Preferences } from "../domain/preferences/schema";
import { Persistence } from "../domain/preferences/persistence";
export const saveMessage = createStore("");
export const preferences = createStore(initial());
export const config = () =>
  preferences.get().profiles.find((p) => p.id === preferences.get().active)!.config;
export const persistence = new Persistence(
  { getItem: (k) => localStorage.getItem(k), setItem: (k, v) => localStorage.setItem(k, v) },
  (message) => saveMessage.set(message),
);
export function initializePreferences() {
  preferences.set(persistence.load());
}
export function setPreferences(p: Preferences) {
  preferences.set(p);
  persistence.schedule(p);
}
export function updateConfig(update: (c: Config) => Config) {
  const p = preferences.get();
  const active = p.profiles.find((profile) => profile.id === p.active)!;
  const next = update(active.config);
  if (next === active.config) return;
  setPreferences({
    ...p,
    profiles: p.profiles.map((profile) =>
      profile.id === p.active ? { ...profile, config: next } : profile,
    ),
  });
}
export function useConfigSelector<T>(select: (c: Config) => T) {
  return useStoreConfig(select);
}
function useStoreConfig<T>(select: (c: Config) => T) {
  return useStore(preferences, (p) => select(p.profiles.find((v) => v.id === p.active)!.config));
}
