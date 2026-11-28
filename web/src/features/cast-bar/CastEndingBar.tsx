import type { CastOutcome } from "../../bridge/snapshot";
import { spellLabel, type SpellKey } from "../../domain/casting";

const OUTCOME_LABEL: Record<CastOutcome, string | null> = {
  completed: null,
  cancelled: "Cancelled",
  interrupted: "Interrupted",
};

/**
 * How a cast just ended, played once on its cast bar (unit-frames.css):
 * completed flashes and fades out, cancelled greys and drains back,
 * interrupted flashes, shakes and holds ember. `progress` is where the
 * bar stood when the cast ended (0–1). Mounted per ending: key it by id.
 */
export function CastEndingBar({
  spell,
  outcome,
  progress,
  label,
  variant = "frame",
}: {
  spell: SpellKey;
  outcome: CastOutcome;
  progress: number;
  /** The bar's accessible name (a frame's; a nameplate's bar is decoration). */
  label: string;
  variant?: "frame" | "nameplate";
}) {
  const from = outcome === "cancelled" ? Math.min(1, Math.max(0, progress)) : 1;
  const text = OUTCOME_LABEL[outcome] ?? spellLabel(spell);
  const fill = { width: `${from * 100}%`, ["--from" as string]: `${from * 100}%` };
  if (variant === "nameplate")
    return (
      <span className={`nameplate-cast cast-ending ${outcome}`} aria-hidden="true">
        <span className="nameplate-cast-fill" style={fill}>
          <span className="cast-tint" />
        </span>
        {outcome === "completed" && <span className="cast-burst" />}
        <span className="nameplate-cast-label">{text}</span>
      </span>
    );
  return (
    <div className={`cast-track cast-ending ${outcome}`} role="status" aria-label={label}>
      <div className="cast-fill" style={fill}>
        <span className="cast-tint" />
      </div>
      {outcome === "completed" && <span className="cast-burst" aria-hidden="true" />}
      <span className="cast-label">
        <span>{text}</span>
        <span />
      </span>
    </div>
  );
}
