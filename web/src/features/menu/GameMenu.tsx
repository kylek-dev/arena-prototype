import { Button, Separator, Text } from "@radix-ui/themes";
import { setMode, setPanel, selectedLayout } from "../../state/session";
import { dispatch } from "../../runtime/commands";
import { debugEnabled, diagnosticsVisible } from "../../runtime/diagnostics";
import { lightingVisible } from "../../state/lighting";

export function GameMenu() {
  return (
    <div className="stack game-menu">
      <Text size="2" color="gray">
        Local solo practice
      </Text>
      <Button onClick={() => setPanel(null)}>Resume</Button>
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
