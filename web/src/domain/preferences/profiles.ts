import { defaultConfig, MAX_PROFILES, type Preferences } from "./schema";
export type ProfileCommand =
  | { type: "create" | "duplicate"; id: string; name: string }
  | { type: "rename"; name: string }
  | { type: "select" | "delete"; id: string }
  | { type: "restore" };
export function profileTransition(p: Preferences, command: ProfileCommand): Preferences {
  const current = p.profiles.find((v) => v.id === p.active)!;
  if (command.type === "create" || command.type === "duplicate") {
    const name = command.name.trim().slice(0, 40);
    if (
      !name ||
      p.profiles.length >= MAX_PROFILES ||
      p.profiles.some((v) => v.id === command.id || v.name === name)
    )
      throw new Error("Choose a unique name; maximum eight profiles.");
    return {
      ...p,
      active: command.id,
      profiles: [
        ...p.profiles,
        {
          id: command.id,
          name,
          config: command.type === "duplicate" ? structuredClone(current.config) : defaultConfig(),
        },
      ],
    };
  }
  if (command.type === "select")
    return p.profiles.some((v) => v.id === command.id) ? { ...p, active: command.id } : p;
  if (command.type === "delete") {
    if (p.profiles.length === 1) throw new Error("Keep at least one profile.");
    const profiles = p.profiles.filter((v) => v.id !== command.id);
    return {
      ...p,
      profiles,
      active: profiles.some((v) => v.id === p.active) ? p.active : profiles[0].id,
    };
  }
  if (command.type === "rename") {
    const name = command.name.trim().slice(0, 40);
    if (!name || p.profiles.some((v) => v.id !== p.active && v.name === name))
      throw new Error("Choose a unique profile name.");
    return { ...p, profiles: p.profiles.map((v) => (v.id === p.active ? { ...v, name } : v)) };
  }
  return {
    ...p,
    profiles: p.profiles.map((v) => (v.id === p.active ? { ...v, config: defaultConfig() } : v)),
  };
}
export function importProfiles(
  existing: Preferences,
  incoming: Preferences,
  makeId: () => string,
): Preferences {
  if (existing.profiles.length + incoming.profiles.length > MAX_PROFILES)
    throw new Error("Import would exceed eight profiles. Delete unused profiles first.");
  const names = new Set(existing.profiles.map((v) => v.name));
  const profiles = incoming.profiles.map((p) => {
    let name = p.name;
    let n = 2;
    while (names.has(name)) name = `${p.name.slice(0, 30)} (${n++})`;
    names.add(name);
    return { ...p, id: makeId(), name };
  });
  return { ...existing, active: profiles[0].id, profiles: [...existing.profiles, ...profiles] };
}
