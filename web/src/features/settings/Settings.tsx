import { useState } from "react";
import { BindingsEditor } from "../keybindings/BindingsEditor";
import { LayoutSettings } from "../layout-editor/LayoutSettings";
import { BarSettings } from "./BarSettings";
import { ProfileSettings } from "./ProfileSettings";
import { GraphicsSettings } from "./GraphicsSettings";
import { Button } from "@radix-ui/themes";
export function Settings() {
  const [tab, setTab] = useState("Bindings");
  return (
    <>
      <nav className="tabs" aria-label="Settings sections">
        {["Bindings", "Action bars", "Layout", "Graphics", "Profiles"].map((v) => (
          <Button
            key={v}
            variant={tab === v ? "soft" : "surface"}
            color="gray"
            aria-pressed={tab === v}
            onClick={() => setTab(v)}
          >
            {v}
          </Button>
        ))}
      </nav>
      {tab === "Bindings" ? (
        <BindingsEditor />
      ) : tab === "Action bars" ? (
        <BarSettings />
      ) : tab === "Layout" ? (
        <LayoutSettings />
      ) : tab === "Graphics" ? (
        <GraphicsSettings />
      ) : (
        <ProfileSettings />
      )}
    </>
  );
}
