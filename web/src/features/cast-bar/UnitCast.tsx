import { useLayoutEffect, useRef } from "react";
import { castProgress, spellLabel, type ActiveCast } from "../../domain/casting";
import { castEndings } from "../../runtime/cast-endings";
import { useStore } from "../../state/store";
import { CastBar } from "./CastBar";
import { CastEndingBar } from "./CastEndingBar";

/**
 * A unit's cast bar: the cast in progress, else for a moment how its last
 * cast ended (completed, cancelled or interrupted), else nothing. `unitId`
 * is the unit's entity id (null for a layout sample, which never ends).
 */
export function UnitCast({
  unitId,
  cast,
  who,
  variant = "frame",
}: {
  unitId: string | null;
  cast: ActiveCast | null;
  /** Names the bar for assistive technology: "player", "target"… */
  who: string;
  variant?: "frame" | "nameplate";
}) {
  const ending = useStore(castEndings, (m) => (unitId ? (m.get(unitId) ?? null) : null));
  // Where this unit's cast stood when last seen, for a cancelled bar to drain from.
  const last = useRef<{ unitId: string | null; cast: ActiveCast } | null>(null);
  useLayoutEffect(() => {
    if (cast) last.current = { unitId, cast };
  });
  if (cast)
    return variant === "nameplate" ? (
      <CastBar key={unitId ?? "sample"} variant="nameplate" cast={cast} />
    ) : (
      <CastBar
        // A different unit (a new target) always snaps.
        key={unitId ?? "sample"}
        variant="frame"
        cast={cast}
        label={`${who} casting ${spellLabel(cast.spell)}`}
      />
    );
  if (!ending) return null;
  const seen = last.current;
  const progress =
    seen && seen.unitId === unitId && seen.cast.spell === ending.spell
      ? castProgress(seen.cast)
      : 0;
  return (
    <CastEndingBar
      key={ending.id}
      spell={ending.spell}
      outcome={ending.outcome}
      progress={progress}
      label={`${who} cast ${spellLabel(ending.spell)} ${ending.outcome}`}
      variant={variant}
    />
  );
}
