import { resourcePercent, spellLabel } from "../../domain/casting";
import { CastBar } from "../cast-bar/CastBar";
import type { Entity } from "../../domain/units/selection";
import type { FrameRole } from "../../domain/preferences/schema";
export function UnitFrame({
  role,
  unit,
  showCast = true,
}: {
  role:
    | Exclude<FrameRole, "opponents" | "team" | "unitTooltip" | "castBar">
    | "opponent"
    | "teammate";
  unit: Entity | null;
  /** False when the player's cast bar is shown in its own frame instead. */
  showCast?: boolean;
}) {
  const hasResource = unit !== null && typeof unit.resource === "number";
  const maxResource = unit?.maxResource ?? 100;
  const resourceValue = hasResource ? (unit.resource as number) : unit ? 100 : 0;
  const resourceWidth = hasResource
    ? resourcePercent(unit.resource as number, maxResource)
    : unit
      ? 100
      : 0;
  const lockedOut = !unit?.disconnected && (unit?.lockoutRemaining ?? 0) > 0;
  const activeCast = showCast ? (unit?.cast ?? null) : null;
  return (
    <section
      className={`unit-frame ${role}${unit?.disconnected ? " disconnected" : ""}`}
      aria-label={`${role} status`}
    >
      <div className="unit-heading">
        <strong title={unit?.name}>{unit?.name ?? "—"}</strong>
        {unit?.disconnected ? (
          <span className="unit-status">Disconnected</span>
        ) : lockedOut ? (
          <span className="unit-status locked-out">Locked out</span>
        ) : null}
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
        aria-valuemax={maxResource}
        aria-valuenow={resourceValue}
        aria-valuetext={
          hasResource
            ? `${resourceValue}/${maxResource}`
            : "Placeholder · resources are not implemented"
        }
      >
        <div className="resource-fill" style={{ width: `${resourceWidth}%` }} />
        <span className="bar-value">{unit ? `${resourceValue}/${maxResource}` : "—"}</span>
      </div>
      {activeCast && (
        <CastBar
          // A different unit in this frame (a new target) always snaps.
          key={unit?.id}
          variant="frame"
          cast={activeCast}
          label={`${role} casting ${spellLabel(activeCast.spell)}`}
        />
      )}
    </section>
  );
}
