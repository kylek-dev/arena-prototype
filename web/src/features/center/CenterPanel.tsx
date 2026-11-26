import type { ReactNode } from "react";
import type { LinkState } from "../../domain/network";
import { networkLink } from "../../state/network";
import { customLobbyVisible, interaction, scoreboardVisible } from "../../state/session";
import { createStore, useStore } from "../../state/store";
import "./center.css";

const NO_LINK = createStore<LinkState>({ phase: "idle", attempt: 0, ended: null, status: null });

/** Whether the player is in a network match (seated or watching). */
export function useInMatch() {
  const link = useStore(networkLink, (l) => l);
  const phase = useStore(link?.state ?? NO_LINK, (s) => s.phase);
  return phase !== "idle" && phase !== "ended";
}

/** Whether the notification panel is free for cards: outside a
 * match, with no centered window open (scoreboard, custom lobby, game menu
 * or settings). Otherwise the queue keeps the top and invitations the
 * right edge, as in a match. */
export function useCenterFree() {
  const inMatch = useInMatch();
  const scoreboard = useStore(scoreboardVisible, (v) => v);
  const lobby = useStore(customLobbyVisible, (v) => v);
  const dialog = useStore(interaction, (s) => s.panel === "menu" || s.panel === "settings");
  return !inMatch && !scoreboard && !lobby && !dialog;
}

/** The top center of the screen: the one place for questions and queue
 * news outside a match (an invitation to accept, a found match, the search
 * and its wait, why a search stopped), stacked as cards from the top.
 * Elsewhere its children keep their own corners. */
export function CenterPanel({ children }: { children: ReactNode }) {
  return (
    <div className="center-panel" role="group" aria-label="Notifications">
      {children}
    </div>
  );
}
