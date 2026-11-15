import { describe, expect, it } from "vitest";
import {
  defaultAppearance,
  parseAppearance,
  parseKits,
  randomAppearance,
  renderSpec,
  selectKit,
  type Appearance,
  type Kit,
} from "../../web/src/domain/appearance";

import { humanKit } from "./kit-fixture";

const none = { id: "none", label: "None", nodes: [], covers: [] };
const elf: Kit = {
  ...humanKit(),
  id: "elf-female",
  ancestry: "elf",
  path: "characters/kits/elf-female.glb",
  regions: ["Body_Full"],
  groups: { hair: [none, { id: "Race_Hair", label: "Locks", nodes: ["Race_Hair"] }] },
  morph: "Body_Full",
  eyes: "elf",
};
const visible = (spec: ReturnType<typeof renderSpec>) =>
  spec.parts.filter(([, shown]) => shown).map(([node]) => node);

describe("character appearance", () => {
  it("default look shows the dressed body and hides covered skin", () => {
    const spec = renderSpec(humanKit(), defaultAppearance);
    expect(spec.kit).toEqual({
      id: "human-female",
      path: "characters/kits/human-female.glb",
      sole: -0.008,
      scale: 1.0656,
    });
    expect(visible(spec)).toEqual(
      [
        "Body_Head",
        "Eyebrows_Female",
        "Hair_Buns",
        "Ranger_Belt_1",
        "Ranger_Body",
        "Ranger_Feet",
        "Ranger_Legs",
      ].sort(),
    );
    // Every node the kit can show is listed, so stale choices are hidden.
    expect(spec.parts.map(([node]) => node)).toContain("Hair_Beard");
    expect(spec.parts.map(([node]) => node)).toContain("Body_Torso");
  });

  it("removing pieces reveals exactly the regions they covered", () => {
    const look: Appearance = {
      ...defaultAppearance,
      groups: { ...defaultAppearance.groups, torso: "none", feet: "none" },
    };
    const shown = visible(renderSpec(humanKit(), look));
    expect(shown).toContain("Body_Torso");
    expect(shown).toContain("Body_Feet");
    expect(shown).not.toContain("Body_Thighs");
  });

  it("a hood hides every hairstyle but keeps brows and beard", () => {
    const look: Appearance = {
      ...defaultAppearance,
      groups: { ...defaultAppearance.groups, head: "Ranger", beard: "Hair_Beard" },
    };
    const shown = visible(renderSpec(humanKit(), look));
    expect(shown).toContain("Ranger_Hood");
    expect(shown).not.toContain("Hair_Buns");
    expect(shown).toContain("Hair_Beard");
    expect(shown).toContain("Eyebrows_Female");
  });

  it("tints and texture variants resolve per material slot", () => {
    const look: Appearance = {
      ...defaultAppearance,
      skinTone: "deep",
      hairColor: "black",
      eyes: "blue",
      variants: { Skin: "light" },
    };
    const materials = Object.fromEntries(
      renderSpec(humanKit(), look).materials.map((m) => [m.name, m]),
    );
    expect(materials.Skin.tint).toEqual([0.36, 0.26, 0.2]);
    expect(materials.Skin.texture).toBe("characters/kits/textures/skin-light.png");
    expect(materials.Hair_2.tint).toEqual([0.1, 0.09, 0.085]);
    expect(materials.Eyes.texture).toBe("characters/kits/textures/eye-blue.png");
    expect(materials.Ranger).toEqual({ name: "Ranger", tint: [1, 1, 1], texture: null });
    // Unknown ids fall back rather than breaking the look.
    const odd = renderSpec(humanKit(), { ...look, skinTone: "x", eyes: "x" }).materials;
    expect(odd.find((m) => m.name === "Skin")?.tint).toEqual([1, 1, 1]);
    expect(odd.find((m) => m.name === "Eyes")?.texture).toContain("eye-brown");
  });

  it("selects kits by ancestry and body, falling back to the human body", () => {
    const kits = [humanKit(), humanKit("male"), elf];
    expect(selectKit(kits, { ...defaultAppearance, body: "male" })?.id).toBe("human-male");
    expect(selectKit(kits, { ...defaultAppearance, ancestry: "elf" })?.id).toBe("elf-female");
    expect(selectKit(kits, { ...defaultAppearance, ancestry: "orc", body: "male" })?.id).toBe(
      "human-male",
    );
    const spec = renderSpec(elf, { ...defaultAppearance, ancestry: "elf", build: 0.4 });
    expect(spec.morph).toEqual({ node: "Body_Full", weight: 0.4 });
    // Missing groups (no outfits on races) fall back to "none".
    expect(visible(spec)).toEqual(["Body_Full"]);
  });

  it("parses stored looks defensively and keeps randomized looks valid", () => {
    expect(parseAppearance(JSON.parse(JSON.stringify(defaultAppearance)))).toEqual(
      defaultAppearance,
    );
    expect(parseAppearance({ ...defaultAppearance, version: 2 })).toBeNull();
    expect(parseAppearance({ ...defaultAppearance, build: Number.NaN })).toBeNull();
    expect(parseAppearance({ ...defaultAppearance, build: 3 })?.build).toBe(1);
    const parsed = parseAppearance({
      ...defaultAppearance,
      groups: { hair: "Hair_Long", bogus: "x", torso: 3 },
    });
    expect(parsed?.groups).toEqual({ hair: "Hair_Long" });
    expect(parseKits([humanKit(), { id: 1 }, null])).toHaveLength(1);
    let seed = 1;
    const random = () => (seed = (seed * 16807) % 2147483647) / 2147483647;
    for (let i = 0; i < 25; i++) {
      const look = randomAppearance([humanKit(), elf], random);
      expect(parseAppearance(look)).toEqual(look);
      expect(() => renderSpec(selectKit([humanKit(), elf], look)!, look)).not.toThrow();
    }
  });
});
