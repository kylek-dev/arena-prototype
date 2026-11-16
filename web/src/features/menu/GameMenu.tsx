import { Button, Separator, Text } from "@radix-ui/themes";
import { setMode, setPanel, selectedLayout } from "../../state/session";
import { dispatch, input } from "../../runtime/commands";
import { engine } from "../../state/session";
import { useStore } from "../../state/store";
import { debugEnabled, diagnosticsVisible } from "../../runtime/diagnostics";
import { lightingVisible } from "../../state/lighting";
import { mapEditorVisible } from "../../state/map-editor";
import { characterKits, customizerVisible } from "../../state/appearance";
import { matchQueue, networkLink } from "../../state/network";
import { createStore } from "../../state/store";
import type { LinkState } from "../../domain/network";
import { IDLE_QUEUE, QUEUE_MODES, modeLabel } from "../../domain/matchmaking";

const NO_LINK = createStore<LinkState>({ phase: "idle", attempt: 0, ended: null, status: null });
const NO_QUEUE = createStore(IDLE_QUEUE);

/** Queue for an online match; while searching, which queue and Cancel. */
function PlayOnline() {
  const queue = useStore(matchQueue, (q) => q);
  const search = useStore(queue?.state ?? NO_QUEUE, (s) => s);
  if (!queue) return null;
  const searching = search.phase === "connecting" || search.phase === "queued";
  return (
    <div className="stack" role="group" aria-labelledby="play-online-heading">
      <Text id="play-online-heading" size="2" weight="medium">
        Play online
      </Text>
      {searching && search.mode ? (
        <>
          <Text size="1" color="gray">
            Searching for a {modeLabel(search.mode)} match. You can keep practicing; the match
            starts when everyone is found.
          </Text>
          <Button variant="soft" color="gray" onClick={() => queue.cancel()}>
            Cancel search
          </Button>
        </>
      ) : search.phase === "found" ? (
        <Text size="1" color="gray">
          Match found · getting ready…
        </Text>
      ) : (
        <div className="queue-modes">
          {QUEUE_MODES.map((mode) => (
            <Button
              key={mode.id}
              variant="surface"
              onClick={() => {
                setPanel(null);
                queue.join(mode.id);
              }}
            >
              {mode.label}
            </Button>
          ))}
        </div>
      )}
    </div>
  );
}

export function GameMenu() {
  const size = useStore(engine, (s) => s?.teamSize ?? 1);
  const customizable = useStore(characterKits, (k) => k.length > 0);
  const link = useStore(networkLink, (l) => l);
  const phase = useStore(link?.state ?? NO_LINK, (s) => s.phase);
  const seat = useStore(link?.state ?? NO_LINK, (s) => s.status?.seat ?? null);
  const inMatch = phase !== "idle" && phase !== "ended";
  const remembered = !inMatch ? link?.remembered() : null;
  return (
    <div className="stack game-menu">
      <Text size="2" color="gray">
        {inMatch
          ? `Network match${seat === null ? "" : ` · seat ${seat + 1}`}`
          : "Local solo practice"}
      </Text>
      <Button onClick={() => setPanel(null)}>Resume</Button>
      {inMatch && link && (
        <Button
          variant="soft"
          color="red"
          onClick={() => {
            setPanel(null);
            link.leave();
          }}
        >
          Leave match
        </Button>
      )}
      {remembered && link && (
        <Button
          variant="surface"
          onClick={() => {
            setPanel(null);
            link.join(remembered);
          }}
        >
          Rejoin last match
        </Button>
      )}
      {!inMatch && <PlayOnline />}
      {!inMatch && (
        <>
          <label className="stack">
            Practice mode
            <select
              aria-label="Practice mode"
              value={size}
              onChange={(event) => {
                const next = Number(event.target.value);
                setPanel(null);
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
        </>
      )}
      <Separator size="4" />
      {customizable && (
        <Button
          variant="surface"
          color="gray"
          onClick={() => {
            setPanel(null);
            customizerVisible.set(true);
          }}
        >
          Customize character
        </Button>
      )}
      <Button variant="surface" color="gray" onClick={() => setPanel("settings")}>
        Settings
      </Button>
      <Button variant="surface" color="gray" onClick={() => setPanel("spellbook")}>
        Spells
      </Button>
      <Button
        variant="surface"
        color="gray"
        onClick={() => {
          setPanel(null);
          lightingVisible.set(true);
        }}
      >
        Lighting tweaks (temporary)
      </Button>
      <Button
        variant="surface"
        color="gray"
        onClick={() => {
          setPanel(null);
          mapEditorVisible.set(true);
        }}
      >
        Map editor
      </Button>
      <Button
        variant="surface"
        color="gray"
        onClick={() => {
          selectedLayout.set(null);
          setMode("layout");
        }}
      >
        Edit layout
      </Button>
      <Button variant="surface" color="gray" onClick={() => setMode("quick")}>
        Quick binding
      </Button>
      {!inMatch && (
        <>
          <Separator size="4" />
          <Button
            variant="soft"
            color="gray"
            onClick={() => {
              setPanel(null);
              dispatch("reset");
            }}
          >
            Reset practice
          </Button>
        </>
      )}
      {debugEnabled && (
        <Button
          variant="ghost"
          color="gray"
          onClick={() => diagnosticsVisible.set(!diagnosticsVisible.get())}
        >
          Toggle diagnostics
        </Button>
      )}
    </div>
  );
}
