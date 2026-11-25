import type { Kit } from "../../web/src/domain/appearance";

const none = { id: "none", label: "None", nodes: [], covers: [] };
const outfit = (slot: string, nodes: string[], covers: string[] = []) => ({
  id: "Ranger",
  label: "Ranger",
  nodes,
  covers,
  slot,
});
export function humanKit(body = "female"): Kit {
  return {
    id: `human-${body}`,
    ancestry: "human",
    body,
    path: `characters/kits/human-${body}.glb`,
    sole: -0.008,
    scale: 1.0656,
    license: "CC0-1.0",
    regions: ["Body_Head", "Body_Torso", "Body_Hips", "Body_Thighs", "Body_Calves", "Body_Feet"],
    groups: {
      hair: [
        none,
        { id: "Hair_Buns", label: "Buns", nodes: ["Hair_Buns"] },
        { id: "Hair_Long", label: "Long", nodes: ["Hair_Long"] },
      ],
      brows: [none, { id: "Eyebrows_Female", label: "Fine", nodes: ["Eyebrows_Female"] }],
      beard: [none, { id: "Hair_Beard", label: "Beard", nodes: ["Hair_Beard"] }],
      head: [none, outfit("head", ["Ranger_Hood"])],
      torso: [none, outfit("torso", ["Ranger_Body", "Ranger_Belt_1"], ["Body_Torso"])],
      legs: [none, outfit("legs", ["Ranger_Legs"], ["Body_Hips", "Body_Thighs", "Body_Calves"])],
      feet: [none, outfit("feet", ["Ranger_Feet"], ["Body_Feet"])],
    },
    materials: {
      Skin: {
        tint: "skin",
        variants: [
          { id: "dark", label: "Dark", texture: null },
          { id: "light", label: "Light", texture: "characters/kits/textures/skin-light.png" },
        ],
      },
      Hair_2: { tint: "hair" },
      Eyes: {
        variants: [
          { id: "brown", label: "Brown", texture: "characters/kits/textures/eye-brown.png" },
          { id: "blue", label: "Blue", texture: "characters/kits/textures/eye-blue.png" },
        ],
      },
      Ranger: { variants: [{ id: "1", label: "Forest", texture: null }] },
    },
    morph: null,
    eyes: "brown",
  };
}

/** A finished kit (ADR 0053): tinted eyes and facial hair, face presets, armour slots. */
export function finishedKit(): Kit {
  return {
    id: "meshy-human-male",
    ancestry: "meshy",
    body: "male",
    path: "characters/kits/meshy-human-male.glb",
    sole: 0,
    scale: 1.0656,
    license: "LicenseRef-Meshy-Pro-private",
    regions: ["Body_Head", "Body_Chest", "Body_Forearms", "Body_Hands", "Trunks", "Body_Feet"],
    groups: {
      face: [
        { id: "chiseled", label: "Chiseled", nodes: [], weights: [0, 0] },
        { id: "broad", label: "Broad", nodes: [], weights: [1, 0] },
        { id: "lean", label: "Lean", nodes: [], weights: [0, 7] },
      ],
      brows: [
        { id: "Brows_Natural", label: "Natural", nodes: ["Brows_Natural"] },
        { id: "Brows_None", label: "None", nodes: [] },
      ],
      beard: [none, { id: "Beard_Full", label: "Full", nodes: ["Beard_Full"] }],
      gloves: [
        none,
        {
          id: "Gladiator",
          label: "Gladiator",
          nodes: ["Gladiator_Gloves"],
          covers: ["Body_Hands"],
        },
      ],
      pants: [
        none,
        { id: "Gladiator", label: "Gladiator", nodes: ["Gladiator_Pants"], covers: ["Trunks"] },
      ],
    },
    materials: {
      Skin: { tint: "skin" },
      Skin_Head: { tint: "skin" },
      Eye_Iris: { tint: "eyes" },
      Brows_Natural: { tint: "hair" },
      FacialHair: { tint: "facialHair" },
    },
    morph: null,
    morphNodes: ["Body_Head", "Brows_Natural"],
    eyes: "brown",
  };
}
