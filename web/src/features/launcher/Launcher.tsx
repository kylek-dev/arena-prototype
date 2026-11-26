import { Hint } from "../../ui/Tooltip";
import { Placed } from "../../ui/Placed";
import { updateConfig, useConfigSelector } from "../../state/config";
import { characterKits, customizerVisible } from "../../state/appearance";
import { networkLink } from "../../state/network";
import { engine, interaction, setPanel } from "../../state/session";
import { createStore, useStore } from "../../state/store";
import { friendsWindowVisible, playWindowVisible } from "../../state/windows";
import type { LinkState } from "../../domain/network";
import "./launcher.css";

const NO_LINK = createStore<LinkState>({ phase: "idle", attempt: 0, ended: null, status: null });

type Entry = {
  letter: string;
  name: string;
  /** The full tooltip: what the window is for, or why it is not available. */
  hint: string;
  open: boolean;
  disabled: boolean;
  toggle: () => void;
};

/** The in-game launcher (playtest inbox): one-letter buttons, each with a
 * full tooltip, that open the Play, Friends, Spells and Customize windows.
 * Movable with the layout editor like the frames. */
export function Launcher() {
  const value = useConfigSelector((c) => c.frames.launcher);
  const editing = useStore(interaction, (s) => s.layout);
  const panel = useStore(interaction, (s) => s.panel);
  const play = useStore(playWindowVisible, (v) => v);
  const friends = useStore(friendsWindowVisible, (v) => v);
  const customizing = useStore(customizerVisible, (v) => v);
  const kits = useStore(characterKits, (k) => k.length > 0);
  const link = useStore(networkLink, (l) => l);
  const phase = useStore(link?.state ?? NO_LINK, (s) => s.phase);
  const statusSpectator = useStore(link?.state ?? NO_LINK, (s) => Boolean(s.status?.spectator));
  const engineSpectating = useStore(engine, (s) => Boolean(s?.spectating));
  const spectating = statusSpectator || engineSpectating;
  const inMatch = phase !== "idle" && phase !== "ended";
  const entries: Entry[] = [
    {
      letter: "P",
      name: "Play",
      hint: "Play: queue for 1v1, 2v2 or 3v3, host or join a custom game, or change and reset practice",
      open: play,
      disabled: false,
      toggle: () => playWindowVisible.set(!play),
    },
    {
      letter: "F",
      name: "Friends",
      hint: "Friends: your friends list; add, invite, remove and block players",
      open: friends,
      disabled: false,
      toggle: () => friendsWindowVisible.set(!friends),
    },
    {
      letter: "S",
      name: "Spells",
      hint: spectating
        ? "Spells: not while spectating"
        : "Spells: drag spells onto your action bars",
      open: panel === "spellbook",
      disabled: spectating,
      toggle: () => setPanel(panel === "spellbook" ? null : "spellbook"),
    },
    {
      letter: "C",
      name: "Customize character",
      hint: !kits
        ? "Customize character: no character is loaded"
        : inMatch
          ? "Customize character: available outside a match (your look is locked for each match)"
          : "Customize character: change your fighter's look",
      open: customizing,
      disabled: !kits || inMatch,
      toggle: () => {
        setPanel(null);
        customizerVisible.set(!customizing);
      },
    },
  ];
  return (
    <Placed
      id="launcher"
      value={value}
      onChange={(next) => updateConfig((c) => ({ ...c, frames: { ...c.frames, launcher: next } }))}
    >
      <nav className="launcher" aria-label="Windows">
        {entries.map((entry) => (
          <Hint key={entry.letter} text={entry.hint} side="left">
            <button
              type="button"
              className="launcher-button"
              aria-label={entry.name}
              aria-pressed={entry.open}
              aria-disabled={entry.disabled || undefined}
              tabIndex={editing ? -1 : 0}
              onClick={() => {
                if (!entry.disabled) entry.toggle();
              }}
            >
              {entry.letter}
            </button>
          </Hint>
        ))}
      </nav>
    </Placed>
  );
}
