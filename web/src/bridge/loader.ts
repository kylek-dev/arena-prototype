import { parseKits } from "../domain/appearance";
import { characterKits } from "../state/appearance";
import { prematch } from "../state/session";
export async function loadEngine() {
  if (!("WebAssembly" in window)) throw new Error("WebAssembly is unavailable.");
  const assetResponse = await fetch(new URL("assets/character.json", document.baseURI));
  if (!assetResponse.ok)
    throw new Error("Asset configuration is missing. Run scripts/build-web.sh.");
  const assetConfig = await assetResponse.json();
  // Builds before BaseFemale only recorded availability; those were BaseMale.
  window.arenaShell.characterModel =
    assetConfig.available === true
      ? typeof assetConfig.model === "string"
        ? assetConfig.model
        : "base-male"
      : undefined;
  // Customizable kits must be known before the engine starts so the first
  // model it loads already wears the saved look.
  const kits = parseKits(assetConfig.kits);
  characterKits.set(kits);
  // A start screen shows the character before the first round; it is
  // already framed behind the loading screen so the close-up warms up.
  if (kits.length) prematch.set(true);
  const url = new URL("pkg/arena_client.js", document.baseURI).href;
  const { default: init } = await import(/* @vite-ignore */ url);
  await init();
}
