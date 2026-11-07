import { layout, type Layout } from "../layout/model";
export type AbilityId = "pulse";
export type Slot = { id: string; ability: AbilityId | null };
export type Bar = { id: string; slots: Slot[]; rows: number; vertical: boolean; layout: Layout };
export const MAX_BARS = 4;
export const MAX_SLOTS = 12;
export function newBar(id: string): Bar {
  return {
    id,
    slots: Array.from({ length: 12 }, (_, i) => ({
      id: `${id}-s${i + 1}`,
      ability: i === 0 ? "pulse" : null,
    })),
    rows: 1,
    vertical: false,
    layout: layout(0.5, 0.92),
  };
}
export type Placement = { ability: AbilityId; source?: string; copy?: boolean };
export function removeAssignment(bars: Bar[], source: string, ability: AbilityId): Bar[] {
  if (!bars.some((b) => b.slots.some((s) => s.id === source && s.ability === ability))) return bars;
  return bars.map((b) => ({
    ...b,
    slots: b.slots.map((s) => (s.id === source ? { ...s, ability: null } : s)),
  }));
}
export function place(bars: Bar[], destination: string, payload: Placement): Bar[] {
  const all = bars.flatMap((b) => b.slots);
  const to = all.find((s) => s.id === destination);
  const from = payload.source ? all.find((s) => s.id === payload.source) : undefined;
  if (
    !to ||
    payload.ability !== "pulse" ||
    (payload.source && (!from || from.ability !== payload.ability)) ||
    from === to
  )
    return bars;
  return bars.map((b) => ({
    ...b,
    slots: b.slots.map((s) =>
      s.id === destination
        ? { ...s, ability: payload.ability }
        : s.id === from?.id && !payload.copy
          ? { ...s, ability: to.ability }
          : s,
    ),
  }));
}
