import { OUT_OF_HUB } from "../../domain/hub";
import { hubScreen } from "../../state/hub-render";
import { matchQueue } from "../../state/network";
import { error, interaction } from "../../state/session";
import { createStore, useStore } from "../../state/store";
import { hubFocus, hubWindowVisible } from "../../state/windows";
import { AdminTag } from "../../ui/AdminTag";
import "./hub.css";

const NO_HUB = createStore(OUT_OF_HUB);

/** Names over the hub's players (ADR 0056), where the renderer says they
 * stand; clicking one opens the H window on that player's menu. */
export function HubNameplates() {
  const queue = useStore(matchQueue, (q) => q);
  const hub = useStore(queue?.hub ?? NO_HUB, (h) => h);
  const anchors = useStore(hubScreen, (a) => a);
  const mode = useStore(interaction, (s) => s);
  const failure = useStore(error, (e) => e);
  if (hub.status !== "in" || failure || mode.layout || mode.panel === "menu") return null;
  return (
    <div className="nameplates hub-nameplates" aria-label="Hub players">
      {anchors.map(({ id, x, y }) => {
        const player = hub.players.get(id);
        if (!player) return null;
        return (
          <button
            key={id}
            type="button"
            className="hub-nameplate"
            style={{ left: `${x * 100}%`, top: `${y * 100}%` }}
            aria-label={`${player.handle}${player.admin ? " (admin)" : ""}. Open their menu`}
            onClick={() => {
              hubWindowVisible.set(true);
              hubFocus.set({ handle: player.handle, at: Date.now() });
            }}
          >
            <span className="hub-nameplate-name">{player.handle}</span>
            {player.admin && <AdminTag compact />}
          </button>
        );
      })}
    </div>
  );
}
