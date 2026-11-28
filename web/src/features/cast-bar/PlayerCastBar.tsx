import type { ActiveCast } from "../../domain/casting";
import { castEndings } from "../../runtime/cast-endings";
import { updateConfig, useConfigSelector } from "../../state/config";
import { engine, interaction } from "../../state/session";
import { useStore } from "../../state/store";
import { Placed } from "../../ui/Placed";
import { UnitCast } from "./UnitCast";

/** Layout-editor sample cast, also used by the player frame when attached. */
export const SAMPLE_CAST: ActiveCast = { spell: "rift_bolt", remainingTicks: 20, totalTicks: 45 };

/**
 * The local player's cast bar as its own movable, scalable frame (default:
 * centred just above the action bars). Hidden while idle, after a moment
 * showing how the last cast ended; the layout editor
 * shows a sample so it can be placed. When "attach to player frame" is on,
 * the player frame shows the bar instead and this renders nothing.
 */
export function PlayerCastBar() {
  const player = useStore(engine, (s) => (s && !s.spectating ? s.player.id : null));
  const cast = useStore(engine, (s) => (s && !s.spectating ? (s.player.cast ?? null) : null));
  const ending = useStore(castEndings, (m) => (player ? m.has(player) : false));
  const editing = useStore(interaction, (s) => s.layout);
  const value = useConfigSelector((c) => c.frames.castBar);
  const attached = useConfigSelector((c) => c.castBarAttached);
  const shown = editing ? SAMPLE_CAST : cast;
  if (attached || (!shown && !(ending && !editing))) return null;
  return (
    <Placed
      id="castBar"
      value={value}
      onChange={(next) => updateConfig((c) => ({ ...c, frames: { ...c.frames, castBar: next } }))}
    >
      <div className={`player-cast-bar${editing ? "" : " player-cast-bar-live"}`}>
        <UnitCast unitId={editing ? null : player} cast={shown} who="player" />
      </div>
    </Placed>
  );
}
