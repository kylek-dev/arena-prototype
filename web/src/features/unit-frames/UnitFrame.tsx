import type { Entity } from "../../domain/units/selection";
import type { FrameRole } from "../../domain/preferences/schema";
export function UnitFrame({
  role,
  unit,
}: {
  role: Exclude<FrameRole, "opponents" | "unitTooltip"> | "opponent";
  unit: Entity | null;
}) {
  return (
    <section className={`unit-frame ${role}`} aria-label={`${role} status`}>
      <div className="unit-heading">
        <strong>{unit?.name ?? "—"}</strong>
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
        <span className="bar-value">{unit ? `${unit.health}/${unit.maxHealth}` : "—"}</span>
      </div>
      <div
        className="resource-track"
        role="meter"
        aria-label={`${role} resource`}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={unit ? 100 : 0}
        aria-valuetext="Placeholder · resources are not implemented"
      >
        <div className="resource-fill" style={{ width: unit ? "100%" : "0%" }} />
        <span className="bar-value">{unit ? "100/100" : "—"}</span>
      </div>
    </section>
  );
}
