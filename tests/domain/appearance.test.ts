import { describe, expect, it } from "vitest";
import {
  defaultAppearance,
  eyeColors,
  hairColors,
  parseAppearance,
  parseKits,
  parseNetworkAppearances,
  practiceAppearances,
  randomAppearance,
  renderSpec,
  resolveRosterSpecs,
  selectKit,
  type Appearance,
  type Kit,
} from "../../web/src/domain/appearance";

import { finishedKit, humanKit } from "./kit-fixture";

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
// The look of the Quaternius kits, the default before the finished kit (ADR 0053) became the only one.
const quaterniusLook: Appearance = {
  ...defaultAppearance,
  body: "female",
  groups: { ...defaultAppearance.groups, hair: "Hair_Buns", brows: "Eyebrows_Female" },
};
const visible = (spec: ReturnType<typeof renderSpec>) =>
  spec.parts.filter(([, shown]) => shown).map(([node]) => node);

describe("character appearance", () => {
  it("default look shows the dressed body and hides covered skin", () => {
    const spec = renderSpec(humanKit(), quaterniusLook);
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
      ...quaterniusLook,
      groups: { ...quaterniusLook.groups, torso: "none", feet: "none" },
    };
    const shown = visible(renderSpec(humanKit(), look));
    expect(shown).toContain("Body_Torso");
    expect(shown).toContain("Body_Feet");
    expect(shown).not.toContain("Body_Thighs");
  });

  it("a hood hides every hairstyle but keeps brows and beard", () => {
    const look: Appearance = {
      ...quaterniusLook,
      groups: { ...quaterniusLook.groups, head: "Ranger", beard: "Hair_Beard" },
    };
    const shown = visible(renderSpec(humanKit(), look));
    expect(shown).toContain("Ranger_Hood");
    expect(shown).not.toContain("Hair_Buns");
    expect(shown).toContain("Hair_Beard");
    expect(shown).toContain("Eyebrows_Female");
  });

  it("tints and texture variants resolve per material slot", () => {
    const look: Appearance = {
      ...quaterniusLook,
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
    expect(materials.Hair_2.tint).toEqual([0.1, 0.09, 0.08]);
    expect(materials.Eyes.texture).toBe("characters/kits/textures/eye-blue.png");
    expect(materials.Ranger).toEqual({ name: "Ranger", tint: [1, 1, 1], texture: null });
    // Unknown ids fall back rather than breaking the look.
    const odd = renderSpec(humanKit(), { ...look, skinTone: "x", eyes: "x" }).materials;
    expect(odd.find((m) => m.name === "Skin")?.tint).toEqual([1, 1, 1]);
    expect(odd.find((m) => m.name === "Eyes")?.texture).toContain("eye-brown");
  });

  it("finished kits tint eyes and facial hair, set face morphs and mask their own regions", () => {
    const kit = finishedKit();
    const look: Appearance = {
      ...quaterniusLook,
      ancestry: "human",
      body: "male",
      hairColor: "black",
      facialHairColor: "ginger",
      eyes: "green",
      groups: { face: "lean", beard: "Beard_Full", gloves: "Gladiator", pants: "Gladiator" },
    };
    const spec = renderSpec(kit, look);
    const tint = (name: string) => spec.materials.find((m) => m.name === name)?.tint;
    expect(tint("Eye_Iris")).toEqual(eyeColors.find((c) => c.id === "green")?.rgb);
    expect(tint("Brows_Natural")).toEqual(hairColors.find((c) => c.id === "black")?.rgb);
    expect(tint("Beard_Full")).toEqual(hairColors.find((c) => c.id === "ginger")?.rgb);
    // Every morph node gets the preset's weights, bounded to 0..1.
    expect(spec.morphs).toEqual([
      { node: "Body_Head", weights: [0, 1] },
      { node: "Brows_Natural", weights: [0, 1] },
      { node: "Hair_SweptBack", weights: [0, 1] },
    ]);
    // The first option of a group without "none" is its default (saved Quaternius brow ids do not apply);
    // a look that names no hairstyle gets the default one.
    expect(visible(spec)).toEqual(
      [
        "Hair_SweptBack",
        "Beard_Full",
        "Body_Chest",
        "Body_Feet",
        "Body_Forearms",
        "Body_Head",
        "Brows_Natural",
        "Gladiator_Gloves",
        "Gladiator_Pants",
      ].sort(),
    );
    // An unknown eye colour falls back to the kit's own; kits without presets send no morphs.
    expect(
      renderSpec(kit, { ...look, eyes: "elf" }).materials.find((m) => m.name === "Eye_Iris")?.tint,
    ).toEqual(eyeColors.find((c) => c.id === "brown")?.rgb);
    expect(renderSpec(kit, { ...look, groups: {} }).morphs[0].weights).toEqual([0, 0]);
    expect(renderSpec(humanKit(), quaterniusLook).morphs).toEqual([]);
    for (let seed = 0; seed < 12; seed += 1) {
      const random = randomAppearance([kit], () => ((seed * 37 + 11) % 97) / 97);
      expect(eyeColors.map((c) => c.id)).toContain(random.eyes);
      expect(random.facialHairColor).toBe(random.hairColor);
    }
  });

  it("the default look is the finished human male with swept-back hair", () => {
    const kits = [finishedKit()];
    expect(selectKit(kits, defaultAppearance)?.id).toBe("meshy-human-male");
    // Looks saved for the retired Quaternius kits still resolve to it.
    expect(selectKit(kits, quaterniusLook)?.id).toBe("meshy-human-male");
    expect(selectKit(kits, { ...defaultAppearance, ancestry: "meshy" })?.id).toBe(
      "meshy-human-male",
    );
    const shown = visible(renderSpec(finishedKit(), defaultAppearance));
    expect(shown).toContain("Hair_SweptBack");
    expect(shown).toContain("Brows_Natural");
    expect(visible(renderSpec(finishedKit(), quaterniusLook))).not.toContain("Hair_Buns");
  });

  it("finished kits show the chosen hairstyle tinted by hair colour and hide it under a helmet", () => {
    const kit = finishedKit();
    kit.groups.head = [none, { id: "Helm", label: "Helm", nodes: ["Helm"], covers: [] }];
    const look: Appearance = {
      ...quaterniusLook,
      ancestry: "human",
      body: "male",
      hairColor: "blonde",
      facialHairColor: "black",
      groups: { hair: "Hair_SweptBack" },
    };
    const spec = renderSpec(kit, look);
    expect(visible(spec)).toContain("Hair_SweptBack");
    expect(spec.materials.find((m) => m.name === "Hair_SweptBack")?.tint).toEqual(
      hairColors.find((c) => c.id === "blonde")?.rgb,
    );
    const bald = renderSpec(kit, { ...look, groups: { hair: "none" } });
    expect(visible(bald)).not.toContain("Hair_SweptBack");
    const helmed = visible(renderSpec(kit, { ...look, groups: { ...look.groups, head: "Helm" } }));
    expect(helmed).toContain("Helm");
    expect(helmed).not.toContain("Hair_SweptBack");
  });

  it("selects kits by ancestry and body, falling back to the human body", () => {
    const kits = [humanKit(), humanKit("male"), elf];
    expect(selectKit(kits, { ...quaterniusLook, body: "male" })?.id).toBe("human-male");
    expect(selectKit(kits, { ...quaterniusLook, ancestry: "elf" })?.id).toBe("elf-female");
    expect(selectKit(kits, { ...quaterniusLook, ancestry: "orc", body: "male" })?.id).toBe(
      "human-male",
    );
    const spec = renderSpec(elf, { ...quaterniusLook, ancestry: "elf", build: 0.4 });
    expect(spec.morph).toEqual({ node: "Body_Full", weight: 0.4 });
    // Missing groups (no outfits on races) fall back to "none".
    expect(visible(spec)).toEqual(["Body_Full"]);
  });

  it("looks saved before facial hair had its own colour use the hair colour", () => {
    const { facialHairColor: _dropped, ...older } = { ...quaterniusLook, hairColor: "ginger" };
    expect(parseAppearance(older)?.facialHairColor).toBe("ginger");
    expect(parseAppearance({ ...older, facialHairColor: "white" })?.facialHairColor).toBe("white");
    expect(parseAppearance({ ...older, facialHairColor: 7 })?.facialHairColor).toBe("ginger");
  });

  it("parses stored looks defensively and keeps randomized looks valid", () => {
    expect(parseAppearance(JSON.parse(JSON.stringify(quaterniusLook)))).toEqual(quaterniusLook);
    expect(parseAppearance({ ...quaterniusLook, version: 2 })).toBeNull();
    expect(parseAppearance({ ...quaterniusLook, build: Number.NaN })).toBeNull();
    expect(parseAppearance({ ...quaterniusLook, build: 3 })?.build).toBe(1);
    const parsed = parseAppearance({
      ...quaterniusLook,
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

  it("generates deterministic practice looks and validates networked looks against the local catalog", () => {
    const kits = [humanKit(), humanKit("male"), elf];
    const p1 = practiceAppearances(kits);
    const p2 = practiceAppearances(kits);
    expect(p1).toHaveLength(5);
    expect(p1).toEqual(p2);

    // Practice roster resolves actor 0 (local) + 5 opponent/teammate looks.
    const practiceSpecs = resolveRosterSpecs(kits, quaterniusLook, p1, null);
    expect(practiceSpecs).toHaveLength(6);
    expect(practiceSpecs[0].kit.id).toBe("human-female");

    // Networked appearances: invalid entries and unknown kits/parts safely fall back to catalog defaults.
    const parsed = parseNetworkAppearances([
      { ...quaterniusLook, body: "male" },
      {
        ...quaterniusLook,
        ancestry: "unknown-race",
        body: "female",
        skinTone: "neon",
        groups: { hair: "Nonexistent" },
      },
      "corrupt-entry",
      null,
    ]);
    expect(parsed).toHaveLength(4);
    expect(parsed?.[2]).toBeNull();
    expect(parsed?.[3]).toBeNull();
    // A hub world's relay has one look per engine actor, any count up to 16.
    expect(parseNetworkAppearances([quaterniusLook])).toHaveLength(1);
    expect(parseNetworkAppearances(Array(16).fill(null))).toHaveLength(16);
    expect(parseNetworkAppearances(Array(17).fill(null))).toBeNull();
    expect(parseNetworkAppearances([])).toBeNull();

    const netSpecs = resolveRosterSpecs(kits, quaterniusLook, p1, parsed);
    expect(netSpecs).toHaveLength(4);
    expect(netSpecs[0].kit.id).toBe("human-male");
    // Unknown ancestry falls back to the human of that body; unknown hair/skin fall back to valid
    // defaults, and missing entries to the default look (the human male).
    expect(netSpecs[1].kit.id).toBe("human-female");
    expect(netSpecs[2].kit.id).toBe("human-male");
    expect(netSpecs[3].kit.id).toBe("human-male");
  });
});
