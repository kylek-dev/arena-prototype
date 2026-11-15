// Character customization (ADR 0040). The build writes a manifest of kits
// (one per body); this module turns the player's choices into the render spec
// the engine applies: visible part nodes, material tints/textures, a morph.
// Presentation only: nothing here affects combat.

export interface KitOption {
  id: string;
  label: string;
  nodes: string[];
  /** Body regions the piece hides (outfit pieces only). */
  covers?: string[];
}
export interface MaterialVariant {
  id: string;
  label: string;
  texture: string | null;
}
export interface KitMaterial {
  tint?: "skin" | "hair";
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
  eyes: string;
}

export type Rgb = readonly [number, number, number];
export interface Swatch {
  id: string;
  label: string;
  rgb: Rgb;
}
// sRGB multipliers over the source textures. The skin atlases are a light
// tan, so "Natural" is the untinted source; lighter tones lift slightly.
export const skinTones: Swatch[] = [
  { id: "porcelain", label: "Porcelain", rgb: [1.1, 1.05, 1.03] },
  { id: "fair", label: "Fair", rgb: [1.04, 1.0, 0.96] },
  { id: "natural", label: "Natural", rgb: [1, 1, 1] },
  { id: "olive", label: "Olive", rgb: [0.88, 0.85, 0.7] },
  { id: "tan", label: "Tan", rgb: [0.82, 0.7, 0.58] },
  { id: "bronze", label: "Bronze", rgb: [0.66, 0.52, 0.4] },
  { id: "brown", label: "Brown", rgb: [0.5, 0.37, 0.28] },
  { id: "deep", label: "Deep", rgb: [0.36, 0.26, 0.2] },
  { id: "ebony", label: "Ebony", rgb: [0.25, 0.18, 0.14] },
];
// The human hair atlases are near-white greys meant to be tinted. Unknown or
// retired ids (such as an earlier "natural") fall back to the untinted atlas.
export const hairColors: Swatch[] = [
  { id: "black", label: "Black", rgb: [0.1, 0.09, 0.085] },
  { id: "dark-brown", label: "Dark brown", rgb: [0.26, 0.17, 0.11] },
  { id: "brown", label: "Brown", rgb: [0.45, 0.3, 0.18] },
  { id: "chestnut", label: "Chestnut", rgb: [0.56, 0.28, 0.14] },
  { id: "auburn", label: "Auburn", rgb: [0.66, 0.25, 0.12] },
  { id: "ginger", label: "Ginger", rgb: [0.9, 0.45, 0.18] },
  { id: "strawberry", label: "Strawberry blonde", rgb: [0.98, 0.66, 0.46] },
  { id: "blonde", label: "Blonde", rgb: [0.98, 0.82, 0.5] },
  { id: "platinum", label: "Platinum", rgb: [1.08, 1.04, 0.92] },
  { id: "grey", label: "Grey", rgb: [0.68, 0.68, 0.68] },
  { id: "white", label: "White", rgb: [1.18, 1.18, 1.18] },
  { id: "crimson", label: "Crimson", rgb: [0.75, 0.08, 0.1] },
  { id: "violet", label: "Violet", rgb: [0.5, 0.25, 0.85] },
  { id: "azure", label: "Azure", rgb: [0.2, 0.5, 1.0] },
  { id: "emerald", label: "Emerald", rgb: [0.15, 0.7, 0.35] },
];

export const outfitSlots = ["head", "shoulders", "torso", "arms", "legs", "feet"] as const;
export const featureGroups = ["hair", "brows", "beard", "horns"] as const;
export type Group = (typeof outfitSlots)[number] | (typeof featureGroups)[number];
export const groupLabels: Record<Group, string> = {
  hair: "Hairstyle",
  brows: "Eyebrows",
  beard: "Facial hair",
  horns: "Horns",
  head: "Head",
  shoulders: "Shoulders",
  torso: "Torso",
  arms: "Arms & hands",
  legs: "Legs",
  feet: "Feet",
};

export interface Appearance {
  version: 1;
  body: string;
  ancestry: string;
  groups: Partial<Record<Group, string>>;
  skinTone: string;
  hairColor: string;
  eyes: string;
  /** Chosen texture variant per kit material slot. */
  variants: Record<string, string>;
  /** Race "Size" morph, 0..1. */
  build: number;
}

// Matches the previous default player: Ranger outfit without the hood, buns.
export const defaultAppearance: Appearance = {
  version: 1,
  body: "female",
  ancestry: "human",
  groups: {
    hair: "Hair_Buns",
    brows: "Eyebrows_Female",
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

const swatch = (list: Swatch[], id: string) =>
  (list.find((s) => s.id === id) ?? list.find((s) => s.id === "natural"))?.rgb ?? [1, 1, 1];

export interface RenderSpec {
  kit: { id: string; path: string; sole: number; scale: number };
  parts: [string, boolean][];
  materials: { name: string; tint: Rgb; texture: string | null }[];
  morph: { node: string; weight: number } | null;
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
          : ([1, 1, 1] as Rgb),
    texture: chosenVariant(kit, appearance, name)?.texture ?? null,
  }));
  return {
    kit: { id: kit.id, path: kit.path, sole: kit.sole, scale: kit.scale },
    parts: [...all].sort().map((node) => [node, shown.has(node)]),
    materials,
    morph: kit.morph ? { node: kit.morph, weight: appearance.build } : null,
  };
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
    hairColor: pick(hairColors.slice(0, 11), random).id,
    eyes: pick(kit.materials.Eyes?.variants?.map((v) => v.id) ?? [kit.eyes], random),
    variants,
    build: kit.morph ? Math.round(random() * 10) / 10 : 0,
  };
}
