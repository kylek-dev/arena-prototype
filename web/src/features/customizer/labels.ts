import { chosen, outfitSlots, type Appearance, type Kit } from "../../domain/appearance";

// Player-facing names. Kit ids are asset node names and never shown directly.
const optionNames: Record<string, string> = {
  none: "None",
  Hair_Buns: "Buns",
  Hair_Long: "Long",
  Hair_SimpleParted: "Side part",
  Hair_Buzzed: "Buzzed",
  Hair_BuzzedFemale: "Cropped",
  Eyebrows_Female: "Fine",
  Eyebrows_Regular: "Bold",
  Hair_Beard: "Full beard",
  Race_Horns: "Horns",
  Ranger: "Ranger",
  Peasant: "Peasant",
};
const raceHair: Record<string, string> = { elf: "Elven locks", orc: "Warrior mane" };
export const ancestryNames: Record<string, string> = {
  human: "Human",
  elf: "Elf",
  orc: "Orc",
};
export const bodyNames: Record<string, string> = { female: "Female", male: "Male" };

export function optionName(kit: Kit, option: { id: string; label: string }) {
  if (option.id === "Race_Hair") return raceHair[kit.ancestry] ?? "Natural";
  return optionNames[option.id] ?? option.label;
}

export function variantName(material: string, index: number, label: string) {
  if (material === "Skin" || material === "Hair_Fantasy") return `Shade ${index + 1}`;
  return label;
}

/** One line such as "Human · Female · Ranger outfit". */
export function describe(kit: Kit, look: Appearance) {
  const parts = [ancestryNames[kit.ancestry] ?? kit.ancestry, bodyNames[kit.body] ?? kit.body];
  const worn = outfitSlots
    .filter((slot) => slot !== "head" && kit.groups[slot])
    .map((slot) => chosen(kit, look, slot)?.id ?? "none");
  const sets = new Set(worn.filter((id) => id !== "none"));
  if (sets.size === 1 && !worn.includes("none"))
    parts.push(`${optionNames[[...sets][0]] ?? [...sets][0]} outfit`);
  else if (sets.size > 0) parts.push("Mixed outfit");
  return parts.join(" · ");
}
