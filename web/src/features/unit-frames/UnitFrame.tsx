import { resourcePercent } from "../../domain/casting";
import { UnitCast } from "../cast-bar/UnitCast";
import { isLowHealth, Spark, useHealthChanges } from "./health-effects";
import type { Entity } from "../../domain/units/selection";
import type { FrameRole } from "../../domain/preferences/schema";

/** Frames drawn mirrored, the art's socket on the right: the units you face. */
const MIRRORED = new Set(["target", "focus", "opponent"]);

export function UnitFrame({
  role,
  unit,
  showCast = true,
  portrait = true,
}: {
  role:
    | Exclude<FrameRole, "opponents" | "team" | "unitTooltip" | "castBar">
    | "opponent"
    | "teammate";
  unit: Entity | null;
  /** False when the player's cast bar is shown in its own frame instead. */
  showCast?: boolean;
  /** False draws the frame without its round portrait socket (a layout
   * setting); the status then sits in the name plaque. */
  portrait?: boolean;
}) {
  const hasResource = unit !== null && typeof unit.resource === "number";
  const maxResource = unit?.maxResource ?? 100;
  const resourceValue = hasResource ? (unit.resource as number) : unit ? 100 : 0;
  const resourceWidth = hasResource
    ? resourcePercent(unit.resource as number, maxResource)
    : unit
      ? 100
      : 0;
  const healthWidth = unit ? Math.min(100, Math.max(0, (unit.health / unit.maxHealth) * 100)) : 0;
  const lockedOut = !unit?.disconnected && (unit?.lockoutRemaining ?? 0) > 0;
  const low = isLowHealth(unit);
  const { trail, heal, endTrail } = useHealthChanges(unit);
  const status = unit?.disconnected ? (
    <span className="unit-status">Disconnected</span>
  ) : lockedOut ? (
    <span className="unit-status locked-out">Locked out</span>
  ) : null;
  return (
    <section
      className={`unit-frame ${role}${MIRRORED.has(role) ? " mirrored" : ""}${portrait ? "" : " bare"}${unit?.disconnected ? " disconnected" : ""}${low ? " low-health" : ""}${heal !== null ? " healing" : ""}`}
      aria-label={`${role} status`}
    >
      <div className="unit-heading">
        <span className="unit-name">
          <strong title={unit?.name}>{unit?.name ?? "—"}</strong>
        </span>
        {!portrait && status}
      </div>
      {/* The art's round socket: a glow in the unit's side's colour, and its
          status when it has one. */}
      {portrait && <span className="unit-socket">{status}</span>}
      <div
        className="health-track"
        role="meter"
        aria-label={`${role} health`}
        aria-valuemin={0}
        aria-valuemax={unit?.maxHealth ?? 100}
        aria-valuenow={unit?.health ?? 0}
      >
        <div className="health-fill" style={{ width: `${healthWidth}%` }}>
          {heal !== null && <span key={heal} className="health-heal-wave" />}
          <Spark percent={healthWidth} />
        </div>
        {trail && trail.from > trail.to && (
          <span
            key={trail.key}
            className="health-trail"
            style={{ left: `${trail.to * 100}%`, width: `${(trail.from - trail.to) * 100}%` }}
            onAnimationEnd={endTrail}
          />
        )}
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
        <div className="resource-fill" style={{ width: `${resourceWidth}%` }}>
          <Spark percent={resourceWidth} />
        </div>
        <span className="bar-value">{unit ? `${resourceValue}/${maxResource}` : "—"}</span>
      </div>
      {showCast && (
        <UnitCast
          unitId={unit && !unit.id.startsWith("sample-") ? unit.id : null}
          cast={unit?.cast ?? null}
          who={role}
        />
      )}
    </section>
  );
}
