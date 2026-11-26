import { Button, Text } from "@radix-ui/themes";
import { setMode, setPanel, selectedLayout } from "../../state/session";
import { engine } from "../../state/session";
import { useStore } from "../../state/store";
import { debugEnabled, diagnosticsVisible } from "../../runtime/diagnostics";
import { mapEditorVisible } from "../../state/map-editor";
import { characterKits, returnToHome } from "../../state/appearance";
import { networkLink } from "../../state/network";
import { createStore } from "../../state/store";
import type { LinkState } from "../../domain/network";

const NO_LINK = createStore<LinkState>({ phase: "idle", attempt: 0, ended: null, status: null });

export function GameMenu() {
  const link = useStore(networkLink, (l) => l);
  const phase = useStore(link?.state ?? NO_LINK, (s) => s.phase);
  const seat = useStore(link?.state ?? NO_LINK, (s) => s.status?.seat ?? null);
  const statusSpectator = useStore(link?.state ?? NO_LINK, (s) => Boolean(s.status?.spectator));
  const engineSpectating = useStore(engine, (s) => Boolean(s?.spectating));
  const spectating = statusSpectator || engineSpectating;
  const inMatch = phase !== "idle" && phase !== "ended";
  const remembered = !inMatch ? link?.remembered() : null;
  // The home screen holds the account, party invites and your fighter.
  const home = useStore(characterKits, (k) => k.length > 0);
  return (
    <div className="stack game-menu">
      <Text size="2" color="gray">
        {inMatch
          ? spectating
            ? "Spectating"
            : `Network match${seat === null ? "" : `, seat ${seat + 1}`}`
          : "Practice"}
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
      {!inMatch && home && (
        <Button variant="surface" onClick={() => returnToHome()}>
          Home screen
        </Button>
      )}
      <Button variant="surface" color="gray" onClick={() => setPanel("settings")}>
        Settings
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
      {!spectating && (
        <>
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
        </>
      )}
      {debugEnabled && (
        <Button
          variant="surface"
          color="gray"
          onClick={() => diagnosticsVisible.set(!diagnosticsVisible.get())}
        >
          Toggle diagnostics
        </Button>
      )}
    </div>
  );
}
