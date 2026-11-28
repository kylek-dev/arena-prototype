// Character customization (ADR 0040). The build writes a manifest of kits
// (one per body); this module turns the player's choices into the render spec
// the engine applies: visible part nodes, material tints/textures, a morph.
// Presentation only: nothing here affects combat.

import { palette, type PaletteId } from "./palette";

export interface KitOption {
  id: string;
  label: string;
  nodes: string[];
  /** Body regions the piece hides (outfit pieces only). */
  covers?: string[];
  /** Morph target weights the option sets on the kit's `morphNodes` (face presets). */
  weights?: number[];
}
export interface MaterialVariant {
  id: string;
  label: string;
  texture: string | null;
}
export interface KitMaterial {
  tint?: "skin" | "hair" | "facialHair" | "eyes";
  variants?: MaterialVariant[];
}
export interface Kit {
  id: string;
  ancestry: string;
  body: string;
  path: string;
  sole: number;
  scale: number;
  license: string;
  regions: string[];
  groups: Record<string, KitOption[]>;
  materials: Record<string, KitMaterial>;
  morph: string | null;
  /** Nodes that carry the face presets' morph targets, all in the same target order. */
  morphNodes?: string[];
  eyes: string;
}

export type Rgb = readonly [number, number, number];
export interface Swatch {
  id: string;
  label: string;
  /** The on-screen colour, from the closed palette (docs/palette.json). */
  color: PaletteId;
  /** sRGB multiplier over the source atlas that lands on `color`. */
  rgb: Rgb;
}

/** sRGB channels (0..1) of a palette swatch. */
export function srgb(id: PaletteId): Rgb {
  const hex = palette[id];
  return [1, 3, 5].map((at) => Number.parseInt(hex.slice(at, at + 2), 16) / 255) as unknown as Rgb;
}

// Approximate average sRGB colour of each tintable atlas. A swatch's tint is
// its palette colour divided by the atlas, so the tinted atlas reads as that
// palette colour and the customizer can show the swatch itself.
export const skinAtlas: Rgb = [0.86, 0.62, 0.48];
export const hairAtlas: Rgb = [0.78, 0.78, 0.76];
/** Average of the grey iris texture of kits whose eye colour is a tint. */
export const irisAtlas: Rgb = [0.53, 0.53, 0.52];
const tone =
  (atlas: Rgb) =>
  (id: string, label: string, color: PaletteId): Swatch => ({
    id,
    label,
    color,
    rgb: srgb(color).map((c, i) => Math.round((c / atlas[i]) * 100) / 100) as unknown as Rgb,
  });
const skin = tone(skinAtlas);
const hair = tone(hairAtlas);
const iris = tone(irisAtlas);

// The skin atlases are a light tan, so "Natural" is the untinted source;
// lighter tones lift slightly.
export const skinTones: Swatch[] = [
  skin("porcelain", "Porcelain", "skin-porcelain"),
  skin("fair", "Fair", "skin-fair"),
  skin("natural", "Natural", "skin-natural"),
  skin("olive", "Olive", "skin-olive"),
  skin("tan", "Tan", "skin-tan"),
  skin("bronze", "Bronze", "skin-bronze"),
  skin("brown", "Brown", "skin-brown"),
  skin("deep", "Deep", "skin-deep"),
  skin("ebony", "Ebony", "skin-ebony"),
];
// The human hair atlases are near-white greys meant to be tinted. Unknown or
// retired ids (such as an earlier "natural") fall back to the untinted atlas.
// Fantasy shades borrow the palette's hue ramps.
export const hairColors: Swatch[] = [
  hair("black", "Black", "hair-black"),
  hair("dark-brown", "Dark brown", "hair-dark-brown"),
  hair("brown", "Brown", "hair-brown"),
  hair("chestnut", "Chestnut", "hair-chestnut"),
  hair("auburn", "Auburn", "hair-auburn"),
  hair("ginger", "Ginger", "hair-ginger"),
  hair("strawberry", "Strawberry blonde", "hair-strawberry"),
  hair("blonde", "Blonde", "hair-blonde"),
  hair("platinum", "Platinum", "hair-platinum"),
  hair("grey", "Grey", "hair-grey"),
  hair("white", "White", "hair-white"),
  hair("crimson", "Crimson", "ember-3"),
  hair("violet", "Violet", "arcane-4"),
  hair("azure", "Azure", "sky-4"),
  hair("emerald", "Emerald", "leaf-4"),
];

// Eye colours for kits whose iris is tinted; the ids match the eye texture
// variants of the other kits, so one saved choice works on every kit. Dark,
// dull steps of each ramp: brighter swatches made the eyes glow in game.
export const eyeColors: Swatch[] = [
  iris("brown", "Brown", "hair-dark-brown"),
  iris("hazel", "Hazel", "sand-3"),
  iris("amber", "Amber", "gold-2"),
  iris("green", "Green", "leaf-2"),
  iris("blue", "Blue", "stone-1"),
  iris("grey", "Grey", "stone-3"),
  iris("violet", "Violet", "arcane-2"),
];

// Equipment slots a kit may offer. "torso", "arms", "legs" and "feet" are the
// outfit pieces of the Quaternius kits; the rest are the armour slots of ADR 0053.
export const outfitSlots = [
  "head",
  "shoulders",
  "back",
  "chest",
  "torso",
  "shirt",
  "tabard",
  "arms",
  "bracers",
  "gloves",
  "belt",
  "legs",
  "pants",
  "feet",
  "boots",
] as const;
export const featureGroups = ["face", "hair", "brows", "beard", "horns"] as const;
export type Group = (typeof outfitSlots)[number] | (typeof featureGroups)[number];
export const groupLabels: Record<Group, string> = {
  face: "Face",
  hair: "Hairstyle",
  brows: "Eyebrows",
  beard: "Facial hair",
  horns: "Horns",
  head: "Head",
  shoulders: "Shoulders",
  back: "Back",
  chest: "Chest",
  torso: "Torso",
  shirt: "Shirt",
  tabard: "Tabard",
  arms: "Arms & hands",
  bracers: "Bracers",
  gloves: "Gloves",
  belt: "Belt",
  legs: "Legs",
  pants: "Pants",
  feet: "Feet",
  boots: "Boots",
};

export interface Appearance {
  version: 1;
  body: string;
  ancestry: string;
  groups: Partial<Record<Group, string>>;
  skinTone: string;
  hairColor: string;
  /** Beard and stubble colour; follows the hair colour until chosen separately. */
  facialHairColor: string;
  eyes: string;
  /** Chosen texture variant per kit material slot. */
  variants: Record<string, string>;
  /** Race "Size" morph, 0..1. */
  build: number;
}

// The finished human male (ADR 0053) with its swept-back hair. The outfit ids
// are the Quaternius kits'; a kit without them shows none.
export const defaultAppearance: Appearance = {
  version: 1,
  body: "male",
  ancestry: "human",
  groups: {
    hair: "Hair_SweptBack",
    brows: "Brows_Natural",
    beard: "none",
    horns: "none",
    head: "none",
    shoulders: "Ranger",
    torso: "Ranger",
    arms: "Ranger",
    legs: "Ranger",
    feet: "Ranger",
  },
  skinTone: "natural",
  hairColor: "auburn",
  facialHairColor: "auburn",
  eyes: "brown",
  variants: {},
  build: 0,
};

const isGroup = (key: string): key is Group =>
  (outfitSlots as readonly string[]).includes(key) ||
  (featureGroups as readonly string[]).includes(key);

export function parseAppearance(value: unknown): Appearance | null {
  if (!value || typeof value !== "object") return null;
  const v = value as Record<string, unknown>;
  const text = (x: unknown) => typeof x === "string" && x.length > 0 && x.length < 64;
  if (
    v.version !== 1 ||
    !text(v.body) ||
    !text(v.ancestry) ||
    !text(v.skinTone) ||
    !text(v.hairColor) ||
    !text(v.eyes) ||
    typeof v.build !== "number" ||
    !Number.isFinite(v.build) ||
    !v.groups ||
    typeof v.groups !== "object" ||
    !v.variants ||
    typeof v.variants !== "object"
  )
    return null;
  const groups: Appearance["groups"] = {};
  for (const [key, choice] of Object.entries(v.groups as Record<string, unknown>))
    if (isGroup(key) && text(choice)) groups[key] = choice as string;
  const variants: Record<string, string> = {};
  for (const [key, choice] of Object.entries(v.variants as Record<string, unknown>))
    if (text(key) && text(choice)) variants[key] = choice as string;
  return {
    version: 1,
    body: v.body as string,
    ancestry: v.ancestry as string,
    groups,
    skinTone: v.skinTone as string,
    hairColor: v.hairColor as string,
    // Looks saved or sent before facial hair had its own colour use the hair colour.
    facialHairColor: (text(v.facialHairColor) ? v.facialHairColor : v.hairColor) as string,
    eyes: v.eyes as string,
    variants,
    build: Math.min(1, Math.max(0, v.build)),
  };
}

export function selectKit(kits: Kit[], appearance: Appearance): Kit | undefined {
  return (
    kits.find((k) => k.ancestry === appearance.ancestry && k.body === appearance.body) ??
    kits.find((k) => k.ancestry === "human" && k.body === appearance.body) ??
    kits[0]
  );
}

/** The chosen option in a group, falling back to the default, then "none". */
export function chosen(kit: Kit, appearance: Appearance, group: Group): KitOption | undefined {
  const options = kit.groups[group];
  if (!options?.length) return undefined;
  const id = appearance.groups[group] ?? defaultAppearance.groups[group];
  return options.find((o) => o.id === id) ?? options.find((o) => o.id === "none") ?? options[0];
}

export function chosenVariant(kit: Kit, appearance: Appearance, material: string) {
  const variants = kit.materials[material]?.variants;
  if (!variants?.length) return undefined;
  const wanted = material === "Eyes" ? appearance.eyes : appearance.variants[material];
  const fallback = material === "Eyes" ? kit.eyes : undefined;
  return (
    variants.find((v) => v.id === wanted) ?? variants.find((v) => v.id === fallback) ?? variants[0]
  );
}

const swatch = (list: Swatch[], id: string, fallback = "natural") =>
  (list.find((s) => s.id === id) ?? list.find((s) => s.id === fallback))?.rgb ?? [1, 1, 1];

export interface RenderSpec {
  kit: { id: string; path: string; sole: number; scale: number };
  parts: [string, boolean][];
  materials: { name: string; tint: Rgb; texture: string | null }[];
  morph: { node: string; weight: number } | null;
  /** Morph target weights per node (face presets); empty when the kit has none. */
  morphs: { node: string; weights: number[] }[];
}

export function renderSpec(kit: Kit, appearance: Appearance): RenderSpec {
  const shown = new Set<string>();
  const covered = new Set<string>();
  const all = new Set<string>(kit.regions);
  for (const [group, options] of Object.entries(kit.groups)) {
    for (const option of options) for (const node of option.nodes) all.add(node);
    if (!isGroup(group)) continue;
    const option = chosen(kit, appearance, group);
    for (const node of option?.nodes ?? []) shown.add(node);
    for (const region of option?.covers ?? []) covered.add(region);
  }
  // A hood is drawn over the head; hair would poke through it.
  if (chosen(kit, appearance, "head")?.id !== "none" && kit.groups.head)
    for (const option of kit.groups.hair ?? []) for (const node of option.nodes) shown.delete(node);
  for (const region of kit.regions) if (!covered.has(region)) shown.add(region);
  const materials = Object.entries(kit.materials).map(([name, material]) => ({
    name,
    tint:
      material.tint === "skin"
        ? swatch(skinTones, appearance.skinTone)
        : material.tint === "hair"
          ? swatch(hairColors, appearance.hairColor)
          : material.tint === "facialHair"
            ? swatch(hairColors, appearance.facialHairColor)
            : material.tint === "eyes"
              ? swatch(eyeColors, appearance.eyes, kit.eyes)
              : ([1, 1, 1] as Rgb),
    texture: chosenVariant(kit, appearance, name)?.texture ?? null,
  }));
  return {
    kit: { id: kit.id, path: kit.path, sole: kit.sole, scale: kit.scale },
    parts: [...all].sort().map((node) => [node, shown.has(node)]),
    materials,
    morph: kit.morph ? { node: kit.morph, weight: appearance.build } : null,
    morphs: faceMorphs(kit, appearance),
  };
}

/** A kit's tinted iris means its eye colours are the shared swatches, not textures. */
export const tintsEyes = (kit: Kit) =>
  Object.values(kit.materials).some((material) => material.tint === "eyes");

function faceMorphs(kit: Kit, appearance: Appearance) {
  const weights = chosen(kit, appearance, "face")?.weights;
  if (!kit.morphNodes?.length || !weights?.every((w) => Number.isFinite(w))) return [];
  const bounded = weights.slice(0, 16).map((w) => Math.min(1, Math.max(0, w)));
  return kit.morphNodes.map((node) => ({ node, weights: bounded }));
}

export function parseKits(value: unknown): Kit[] {
  if (!Array.isArray(value)) return [];
  return value.filter(
    (k): k is Kit =>
      !!k &&
      typeof k === "object" &&
      typeof k.id === "string" &&
      typeof k.path === "string" &&
      typeof k.ancestry === "string" &&
      typeof k.body === "string" &&
      Number.isFinite(k.sole) &&
      Number.isFinite(k.scale) &&
      Array.isArray(k.regions) &&
      !!k.groups &&
      typeof k.groups === "object" &&
      !!k.materials &&
      typeof k.materials === "object",
  );
}

const pick = <T>(list: readonly T[], random: () => number) =>
  list[Math.floor(random() * list.length) % list.length];

/** A random but coherent look within the installed kits. */
export function randomAppearance(kits: Kit[], random = Math.random): Appearance {
  const kit = pick(kits, random);
  const groups: Appearance["groups"] = {};
  for (const [group, options] of Object.entries(kit.groups))
    if (isGroup(group)) groups[group] = pick(options, random).id;
  // Mostly dressed: an outfit set is chosen as a whole, hood rarely.
  const outfits = (kit.groups.torso ?? []).filter((o) => o.id !== "none").map((o) => o.id);
  if (outfits.length) {
    const outfit = pick(outfits, random);
    for (const slot of outfitSlots)
      if (kit.groups[slot]?.some((o) => o.id === outfit))
        groups[slot] = slot === "head" && random() > 0.2 ? "none" : outfit;
  }
  const variants: Record<string, string> = {};
  for (const [name, material] of Object.entries(kit.materials))
    if (material.variants?.length && name !== "Eyes")
      variants[name] = pick(material.variants, random).id;
  return {
    version: 1,
    body: kit.body,
    ancestry: kit.ancestry,
    groups,
    skinTone: pick(skinTones, random).id,
    ...naturalHair(random),
    eyes: pick(
      tintsEyes(kit)
        ? eyeColors.map((c) => c.id)
        : (kit.materials.Eyes?.variants?.map((v) => v.id) ?? [kit.eyes]),
      random,
    ),
    variants,
    build: kit.morph ? Math.round(random() * 10) / 10 : 0,
  };
}

// A natural shade; the beard matches the hair.
function naturalHair(random: () => number) {
  const hairColor = pick(hairColors.slice(0, 11), random).id;
  return { hairColor, facialHairColor: hairColor };
}

/** Deterministic PRNG (mulberry32) for stable, distinct practice opponent looks. */
function seededRandom(seed: number): () => number {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = Math.imul(state ^ (state >>> 15), 1 | state);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** Coherent, varied looks for non-player practice slots (`1..=5`). */
export function practiceAppearances(kits: Kit[], count = 5, seed = 0x51a7_2026): Appearance[] {
  if (!kits.length) return [];
  const random = seededRandom(seed);
  return Array.from({ length: count }, () => randomAppearance(kits, random));
}

/** A match's looks relay has one look per seat (2, 4 or 6); a hub world's
 * (ADR 0057) one per engine actor, up to 44. */
const validRosterCount = (count: number) =>
  [2, 4, 6].includes(count) || (count >= 1 && count <= 44);

/** Parses `onAppearances` relayed from Rust (`null` when not in a network match,
 * otherwise an even-length `2 | 4 | 6` array of validated looks or `null` fallbacks). */
export function parseNetworkAppearances(raw: unknown): (Appearance | null)[] | null {
  if (raw === null || raw === undefined) return null;
  if (!Array.isArray(raw) || !validRosterCount(raw.length)) return null;
  return raw.map((entry) => parseAppearance(entry));
}

/** Resolves render specs for every fighter slot (`0..6`) against the local kit catalog.
 * Unknown kits, parts, colors, or variants safely fall back to catalog defaults. */
export function resolveRosterSpecs(
  kits: Kit[],
  localLook: Appearance,
  practiceLooks: readonly Appearance[],
  remoteLooks: readonly (Appearance | null)[] | null,
): RenderSpec[] {
  if (!kits.length) return [];
  const resolveOne = (look: Appearance): RenderSpec | null => {
    const kit = selectKit(kits, look);
    return kit ? renderSpec(kit, look) : null;
  };
  if (remoteLooks !== null) {
    return remoteLooks
      .map((look) => resolveOne(look ?? defaultAppearance))
      .filter((spec): spec is RenderSpec => spec !== null);
  }
  const specs: RenderSpec[] = [];
  const own = resolveOne(localLook);
  if (own) specs.push(own);
  for (let i = 0; i < 5; i += 1) {
    const spec = resolveOne(practiceLooks[i] ?? defaultAppearance);
    if (spec) specs.push(spec);
  }
  return specs;
}
