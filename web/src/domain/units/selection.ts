export type Entity = { id: string; name: string; health: number; maxHealth: number };
export function reconcileFocus(focus: string | null, entities: Entity[], reset = false) {
  return !reset && entities.some((e) => e.id === focus) ? focus : null;
}
export function nextTarget(ids: string[], current: string | null): string | null {
  const ordered = [...ids].sort();
  return ordered.length ? ordered[(ordered.indexOf(current ?? "") + 1) % ordered.length] : null;
}
