import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { defaultAppearance } from "../../web/src/domain/appearance";
import { Customizer } from "../../web/src/features/customizer/Customizer";
import { GameMenu } from "../../web/src/features/menu/GameMenu";
import { Launcher } from "../../web/src/features/launcher/Launcher";
import { TooltipProvider } from "../../web/src/ui/Tooltip";
import {
  appearance,
  characterKits,
  customizerVisible,
  preview,
  readAppearance,
  readPreview,
} from "../../web/src/state/appearance";
import { blocksGameplay, setPanel } from "../../web/src/state/session";
import { finishedKit, humanKit } from "../domain/kit-fixture";

beforeEach(() => {
  localStorage.clear();
  characterKits.set([humanKit(), humanKit("male")]);
  appearance.set({ ...defaultAppearance });
  customizerVisible.set(false);
  preview.set({ yaw: 0, zoom: 0 });
  setPanel(null);
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const open = () => {
  customizerVisible.set(true);
  return render(<Customizer />, { wrapper: TooltipProvider });
};

it("opens from the launcher's C during play and closes with Done", () => {
  render(
    <TooltipProvider>
      <Launcher />
      <Customizer />
    </TooltipProvider>,
  );
  fireEvent.click(screen.getByRole("button", { name: "Customize character" }));
  const panel = screen.getByRole("complementary", { name: "Appearance" });
  expect(within(panel).queryByRole("button", { name: "Enter the arena" })).toBeNull();
  expect(blocksGameplay()).toBe(true);
  expect(readPreview()).toEqual([1, 0, 0]);
  fireEvent.click(screen.getByRole("button", { name: "Face" }));
  fireEvent.click(screen.getByRole("button", { name: "Turn right" }));
  expect(readPreview()[2]).toBe(1);
  expect(readPreview()[1]).toBeCloseTo(Math.PI / 4);
  fireEvent.click(screen.getByRole("button", { name: "Done" }));
  expect(customizerVisible.get()).toBe(false);
  expect(blocksGameplay()).toBe(false);
  expect(readPreview()[0]).toBe(0);
});

it("the menu offers no character screen without installed kits", () => {
  characterKits.set([]);
  setPanel("menu");
  render(<GameMenu />);
  expect(screen.queryByRole("button", { name: "Customize character" })).toBeNull();
});

it("shows player-facing names and updates the saved look and engine spec", () => {
  customizerVisible.set(true);
  render(<Customizer />);
  const [first] = readAppearance() as [number, string];
  fireEvent.click(
    within(screen.getByRole("radiogroup", { name: "Body type" })).getByRole("radio", {
      name: "Male",
    }),
  );
  const [second, json] = readAppearance() as [number, string];
  expect(second).toBeGreaterThan(first);
  expect(JSON.parse(json).kit.path).toBe("characters/kits/human-male.glb");
  expect(readAppearance()[0]).toBe(second);

  fireEvent.click(screen.getByRole("tab", { name: "Head" }));
  const hair = screen.getByRole("radiogroup", { name: "Hairstyle" });
  expect(within(hair).queryByText(/Hair_/)).toBeNull();
  fireEvent.click(within(hair).getByRole("radio", { name: "Long" }));
  fireEvent.click(screen.getByRole("radio", { name: "Black" }));
  fireEvent.click(
    within(screen.getByRole("radiogroup", { name: "Eyes" })).getByRole("radio", { name: "Blue" }),
  );
  expect(appearance.get()).toMatchObject({
    groups: expect.objectContaining({ hair: "Hair_Long" }),
    hairColor: "black",
    eyes: "blue",
  });

  fireEvent.click(screen.getByRole("tab", { name: "Outfit" }));
  fireEvent.click(screen.getByRole("button", { name: "Unequip all" }));
  const parts = JSON.parse((readAppearance() as [number, string])[1]).parts as [string, boolean][];
  expect(parts.filter(([, shown]) => shown).map(([n]) => n)).toContain("Body_Torso");
  fireEvent.click(screen.getByRole("button", { name: "Ranger set" }));
  expect(appearance.get().groups).toMatchObject({ torso: "Ranger", legs: "Ranger", head: "none" });
  fireEvent.click(screen.getByRole("button", { name: "Reset" }));
  expect(appearance.get()).toEqual(defaultAppearance);
});

it("finished kits choose a face, an eye colour and a separate facial hair colour", () => {
  characterKits.set([finishedKit()]);
  appearance.set({ ...defaultAppearance, ancestry: "human", body: "male" });
  customizerVisible.set(true);
  render(<Customizer />);
  fireEvent.click(screen.getByRole("tab", { name: "Head" }));
  const choose = (group: string, name: string) =>
    fireEvent.click(
      within(screen.getByRole("radiogroup", { name: group })).getByRole("radio", { name }),
    );
  choose("Face", "Broad");
  choose("Eye colour", "Green");
  // The beard follows the hair colour until it has been given its own.
  choose("Hair colour", "Black");
  expect(appearance.get()).toMatchObject({
    hairColor: "black",
    facialHairColor: "black",
    eyes: "green",
    groups: expect.objectContaining({ face: "broad" }),
  });
  choose("Facial hair colour", "Ginger");
  choose("Hair colour", "Blonde");
  expect(appearance.get()).toMatchObject({ hairColor: "blonde", facialHairColor: "ginger" });
  const spec = JSON.parse((readAppearance() as [number, string])[1]);
  expect(spec.morphs).toEqual([
    { node: "Body_Head", weights: [1, 0] },
    { node: "Brows_Natural", weights: [1, 0] },
    { node: "Hair_SweptBack", weights: [1, 0] },
  ]);
  // Eye colour is a tint on these kits, not a texture list.
  expect(screen.queryByRole("radiogroup", { name: "Eyes" })).toBeNull();
});

it("dragging the preview turns the character without selecting text", () => {
  open();
  const stage = screen.getByText(/Drag to turn/).parentElement as HTMLElement;
  // fireEvent returns false when the handler prevented the default (text selection, native drag).
  expect(fireEvent.pointerDown(stage, { clientX: 100, pointerId: 1 })).toBe(false);
  fireEvent.pointerMove(stage, { clientX: 150, pointerId: 1 });
  expect(preview.get().yaw).toBeCloseTo(0.5);
  fireEvent.pointerUp(stage, { pointerId: 1 });
  fireEvent.pointerMove(stage, { clientX: 300, pointerId: 1 });
  expect(preview.get().yaw).toBeCloseTo(0.5);
  // The view buttons keep their normal clicks.
  expect(fireEvent.pointerDown(screen.getByRole("button", { name: "Face" }))).toBe(true);
});
