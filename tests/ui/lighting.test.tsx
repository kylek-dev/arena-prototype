import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { defaultLighting, lightingControls, patchLighting } from "../../web/src/domain/lighting";
import { LightingTweaks } from "../../web/src/features/lighting-tweaks/LightingTweaks";
import { GameMenu } from "../../web/src/features/menu/GameMenu";
import { GraphicsSettings } from "../../web/src/features/settings/GraphicsSettings";
import {
  lighting,
  lightingVisible,
  readLighting,
  resetLighting,
} from "../../web/src/state/lighting";
import { defaultGraphics } from "../../web/src/domain/graphics";
import { graphics } from "../../web/src/state/graphics";
import { interaction, setPanel } from "../../web/src/state/session";

beforeEach(() => {
  resetLighting();
  lightingVisible.set(false);
  graphics.set({ ...defaultGraphics });
  setPanel(null);
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

it("opens the temporary nonmodal window from the menu and updates every slider callback live", () => {
  setPanel("menu");
  render(
    <>
      <GameMenu />
      <LightingTweaks />
    </>,
  );
  expect(screen.queryByRole("complementary")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Lighting tweaks (temporary)" }));
  expect(interaction.get().panel).toBeNull();
  const panel = screen.getByRole("complementary", { name: "Lighting tweaks (temporary)" });
  expect(panel.closest(".scrim")).toBeNull();
  expect(screen.getAllByRole("slider")).toHaveLength(28);
  for (const [index, control] of lightingControls.slice(0, -1).entries()) {
    const slider = screen.getByRole("slider", { name: control.label });
    const value = control.min + (control.max - control.min) / 2;
    fireEvent.change(slider, { target: { value: String(value) } });
    expect(readLighting()[index]).toBe(value);
  }
  fireEvent.change(screen.getByRole("combobox", { name: "Tone mapping" }), {
    target: { value: "2" },
  });
  expect(readLighting()[28]).toBe(2);
  expect(
    JSON.parse(
      (screen.getByRole("textbox", { name: "Current lighting values" }) as HTMLTextAreaElement)
        .value,
    ),
  ).toEqual(lighting.get());
  fireEvent.click(screen.getByRole("button", { name: "Reset lighting tweaks" }));
  expect(lighting.get()).toEqual(defaultLighting);
  expect(readLighting()).toEqual(lightingControls.map((control) => control.initial));
  expect(readLighting().slice(0, 10)).toEqual([
    0.18, 0.21, 0.25, 500, 6500, 3200, 10, 0.22, 0.8, 0.4,
  ]);
});

it("retains session tweaks on close, reopens through Graphics and closes with Escape from a slider", () => {
  render(
    <>
      <GraphicsSettings />
      <LightingTweaks />
    </>,
  );
  fireEvent.click(screen.getByRole("button", { name: "Open lighting tweaks (temporary)" }));
  fireEvent.change(screen.getByRole("slider", { name: "Exposure (EV100)" }), {
    target: { value: "8.5" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Close Lighting tweaks (temporary)" }));
  expect(screen.queryByRole("slider")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Open lighting tweaks (temporary)" }));
  expect((screen.getByRole("slider", { name: "Exposure (EV100)" }) as HTMLInputElement).value).toBe(
    "8.5",
  );
  fireEvent.keyDown(screen.getByRole("slider", { name: "Exposure (EV100)" }), {
    key: "Escape",
    code: "Escape",
  });
  expect(screen.queryByRole("slider")).toBeNull();
});

it("does not persist temporary sliders and explains disabled bloom", () => {
  const save = vi.spyOn(Storage.prototype, "setItem");
  lightingVisible.set(true);
  graphics.set({ ...defaultGraphics, bloom: false });
  render(<LightingTweaks />);
  expect(screen.getByRole("status").textContent).toContain("Bloom is off");
  fireEvent.change(screen.getByRole("slider", { name: "Bloom intensity" }), {
    target: { value: "0.7" },
  });
  expect(save).not.toHaveBeenCalled();
  expect(readLighting()[7]).toBe(0.7);
  fireEvent.click(screen.getByRole("checkbox", { name: "Enable bloom (device setting)" }));
  expect(graphics.get().bloom).toBe(true);
  expect(screen.queryByRole("status")).toBeNull();
});

it("rejects non-finite/unknown changes and clamps finite values", () => {
  expect(patchLighting(defaultLighting, "ambient", Number.NaN)).toBe(defaultLighting);
  expect(patchLighting(defaultLighting, "unknown" as "ambient", 1)).toBe(defaultLighting);
  expect(patchLighting(defaultLighting, "exposure", 100).exposure).toBe(14);
  expect(patchLighting(defaultLighting, "ambient", -100).ambient).toBe(0);
  expect(patchLighting(defaultLighting, "tonemapping", 1.7).tonemapping).toBe(2);
});
