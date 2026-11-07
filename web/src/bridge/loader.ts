export async function loadEngine() {
  if (!("WebAssembly" in window)) throw new Error("WebAssembly is unavailable.");
  const assetResponse = await fetch(new URL("assets/character.json", document.baseURI));
  if (!assetResponse.ok)
    throw new Error("Asset configuration is missing. Run scripts/build-web.sh.");
  const assetConfig = await assetResponse.json();
  window.arenaShell.hasCharacterModel = assetConfig.available === true;
  const url = new URL("pkg/arena_client.js", document.baseURI).href;
  const { default: init } = await import(/* @vite-ignore */ url);
  await init();
}
