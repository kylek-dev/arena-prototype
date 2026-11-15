export type CombatTextProjection = {
  id: number;
  source: string;
  target: string;
  delta: number;
  x: number;
  y: number;
  progress: number;
};

export function validCombatText(value: CombatTextProjection): boolean {
  return (
    !!value &&
    Number.isSafeInteger(value.id) &&
    value.id > 0 &&
    typeof value.source === "string" &&
    typeof value.target === "string" &&
    Number.isSafeInteger(value.delta) &&
    value.delta !== 0 &&
    [value.x, value.y, value.progress].every((n) => Number.isFinite(n) && n >= 0 && n <= 1)
  );
}

export function ownOutgoingText(entries: CombatTextProjection[]) {
  return entries.filter(
    (entry) =>
      validCombatText(entry) &&
      entry.source === "Player" &&
      /^(Opponent|Team)[1-3]$/.test(entry.target),
  );
}
