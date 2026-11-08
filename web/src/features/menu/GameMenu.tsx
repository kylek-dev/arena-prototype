import { Button, Separator, Text } from "@radix-ui/themes";
import { setMode, setPanel, selectedLayout } from "../../state/session";
import { dispatch, input } from "../../runtime/commands";
import { engine } from "../../state/session";
import { useStore } from "../../state/store";
import { debugEnabled, diagnosticsVisible } from "../../runtime/diagnostics";
import { lightingVisible } from "../../state/lighting";

export function GameMenu() {
  const size = useStore(engine, (s) => s?.teamSize ?? 1);
  return (
    <div className="stack game-menu">
      <Text size="2" color="gray">
        Local solo practice
      </Text>
      <Button onClick={() => setPanel(null)}>Resume</Button>
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
      <Separator size="4" />
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
          selectedLayout.set(null);
          setMode("layout");
        }}
      >
        Edit layout
      </Button>
      <Button variant="surface" color="gray" onClick={() => setMode("quick")}>
        Quick binding
      </Button>
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
