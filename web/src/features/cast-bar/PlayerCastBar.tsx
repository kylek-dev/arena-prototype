import type { ActiveCast } from "../../domain/casting";
import { spellLabel } from "../../domain/casting";
import { updateConfig, useConfigSelector } from "../../state/config";
import { engine, interaction } from "../../state/session";
import { useStore } from "../../state/store";
import { Placed } from "../../ui/Placed";
import { CastBar } from "./CastBar";

/** Layout-editor sample cast, also used by the player frame when attached. */
export const SAMPLE_CAST: ActiveCast = { spell: "rift_bolt", remainingTicks: 20, totalTicks: 45 };

/**
 * The local player's cast bar as its own movable, scalable frame (default:
 * centred just above the action bars). Hidden while idle; the layout editor
 * shows a sample so it can be placed. When "attach to player frame" is on,
 * the player frame shows the bar instead and this renders nothing.
 */
export function PlayerCastBar() {
  const cast = useStore(engine, (s) => (s && !s.spectating ? (s.player.cast ?? null) : null));
  const editing = useStore(interaction, (s) => s.layout);
  const value = useConfigSelector((c) => c.frames.castBar);
  const attached = useConfigSelector((c) => c.castBarAttached);
  const shown = editing ? SAMPLE_CAST : cast;
  if (attached || !shown) return null;
  return (
    <Placed
      id="castBar"
      value={value}
      onChange={(next) => updateConfig((c) => ({ ...c, frames: { ...c.frames, castBar: next } }))}
    >
      <div className={`player-cast-bar${editing ? "" : " player-cast-bar-live"}`}>
        <CastBar
          key={editing ? "sample" : "live"}
          variant="frame"
          cast={shown}
          label={`player casting ${spellLabel(shown.spell)}`}
        />
      </div>
    </Placed>
  );
}
