import type { Entity } from "../../domain/units/selection";
import type { FrameRole } from "../../domain/preferences/schema";
export function UnitFrame({
  role,
  unit,
  selected,
  focused,
  preview,
}: {
  role: FrameRole;
  unit: Entity | null;
  selected?: boolean;
  focused?: boolean;
  preview?: boolean;
}) {
  return (
    <section className={`unit-frame ${role}`} aria-label={`${role} status`}>
      <div className="eyebrow">
        {role}
        {selected ? " · selected" : ""}
        {focused ? " · focus" : ""}
      </div>
      <div className="unit-heading">
        <strong>{unit?.name ?? (preview ? `${role} preview` : `No ${role}`)}</strong>
        <span>{unit ? `${unit.health} / ${unit.maxHealth}` : "—"}</span>
      </div>
      <div
        className="health-track"
        role="meter"
        aria-label={`${role} health`}
        aria-valuemin={0}
        aria-valuemax={unit?.maxHealth ?? 100}
        aria-valuenow={unit?.health ?? 0}
      >
        <div
          className="health-fill"
          style={{ width: `${unit ? (unit.health / unit.maxHealth) * 100 : 0}%` }}
        />
      </div>
      <small>
        {preview
          ? "Layout sample"
          : unit
            ? unit.health === 0
              ? "Defeated"
              : role === "player"
                ? "Equalized practice loadout"
                : "Practice opponent"
            : "Select an opponent to begin"}
      </small>
    </section>
  );
}
