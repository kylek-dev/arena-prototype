import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { defaultLighting, lightingControls, patchLighting } from "../../web/src/domain/lighting";
import { GameMenu } from "../../web/src/features/menu/GameMenu";
import { GraphicsSettings } from "../../web/src/features/settings/GraphicsSettings";
import { lighting, readLighting } from "../../web/src/state/lighting";
import { setPanel } from "../../web/src/state/session";
import lightingPreset from "../../assets/lighting.json";

afterEach(() => {
  cleanup();
  setPanel(null);
});

it("feeds the renderer the authored preset", () => {
  expect(lighting.get()).toEqual(defaultLighting);
  expect(defaultLighting).toEqual(lightingPreset);
  expect(readLighting()).toEqual(lightingControls.map((control) => control.initial));
  expect(readLighting().slice(0, 10)).toEqual([
    0.18, 0.21, 0.25, 500, 6500, 200, 8.5, 0.22, 0.8, 0.4,
  ]);
});

it("offers no lighting tweaks window anywhere (removed in playtest)", () => {
  setPanel("menu");
  render(
    <>
      <GameMenu />
      <GraphicsSettings />
    </>,
  );
  expect(screen.queryByRole("button", { name: /lighting tweaks/i })).toBeNull();
});

it("rejects non-finite/unknown changes and clamps finite values", () => {
  expect(patchLighting(defaultLighting, "ambient", Number.NaN)).toBe(defaultLighting);
  expect(patchLighting(defaultLighting, "unknown" as "ambient", 1)).toBe(defaultLighting);
  expect(patchLighting(defaultLighting, "exposure", 100).exposure).toBe(14);
  expect(patchLighting(defaultLighting, "ambient", -100).ambient).toBe(0);
  expect(patchLighting(defaultLighting, "tonemapping", 1.7).tonemapping).toBe(2);
});
