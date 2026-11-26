import { Button, Separator, Text } from "@radix-ui/themes";
import type { LinkState } from "../../domain/network";
import { dispatch, input } from "../../runtime/commands";
import { networkLink } from "../../state/network";
import { engine } from "../../state/session";
import { createStore, useStore } from "../../state/store";
import { playWindowVisible } from "../../state/windows";
import { MovableWindow } from "../../ui/MovableWindow";
import { PlayOnline } from "../play-online/PlayOnline";

const NO_LINK = createStore<LinkState>({ phase: "idle", attempt: 0, ended: null, status: null });

/** Practice: its team size and a fresh round. */
function Practice() {
  const size = useStore(engine, (s) => s?.teamSize ?? 1);
  return (
    <section className="stack" aria-labelledby="practice-heading">
      <Text id="practice-heading" size="2" weight="medium">
        Practice
      </Text>
      <label className="stack">
        Practice mode
        <select
          aria-label="Practice mode"
          value={size}
          onChange={(event) => {
            const next = Number(event.target.value);
            dispatch("reset");
            input.teamSize = next;
          }}
        >
          <option value="1">1v1</option>
          <option value="2">2v2</option>
          <option value="3">3v3</option>
        </select>
      </label>
      <Text size="1" color="gray">
        Changing mode starts a fresh round. Other units are stationary.
      </Text>
      <Button variant="soft" color="gray" onClick={() => dispatch("reset")}>
        Reset practice
      </Button>
    </section>
  );
}

/** P: queue for each bracket, custom games, and practice (playtest inbox). */
export function PlayWindow() {
  const visible = useStore(playWindowVisible, (v) => v);
  const link = useStore(networkLink, (l) => l);
  const phase = useStore(link?.state ?? NO_LINK, (s) => s.phase);
  if (!visible) return null;
  const inMatch = phase !== "idle" && phase !== "ended";
  const close = () => playWindowVisible.set(false);
  return (
    <MovableWindow title="Play" close={close} className="play-window" anchor={{ x: 0.6, y: 0.2 }}>
      {inMatch ? (
        <Text size="2" color="gray">
          You are in a match. Leave it from the game menu to queue or practice.
        </Text>
      ) : (
        <div className="stack">
          <PlayOnline inGame onQueue={close} />
          <Separator size="4" />
          <Practice />
        </div>
      )}
    </MovableWindow>
  );
}
