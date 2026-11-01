# Optional stock character

The local player can use **Undead, Grim Reaper, Dark Fantasy** by **PixelLabs**:

- Source: https://pixabay.com/3d-models/undead-grim-reaper-dark-fantasy-glb-3417/
- License: [Pixabay Content License summary](https://pixabay.com/service/license-summary/) and [full terms](https://pixabay.com/service/terms/).
- Source listing labels this model AI-generated; the underlying generation tool/prompt was not supplied.
- Expected local filename: `pixellabs-undead-3417.glb` in this directory.

This is third-party stock art used as a prototype reference, not original project art or an MIT-licensed contribution. The license restricts distribution of the unchanged asset as a standalone file. The raw GLB is therefore excluded from public Git history. Obtain it directly from its source under those terms and place it here to enable it locally. The source file supplied by the project owner is preserved unchanged; its identity and technical inspection are recorded in manifest.json.

Run `./scripts/build-web.sh` after adding or removing the file. The build copies it into ignored web/assets/characters and writes web/assets/character.json. Without the optional download, clean clones and CI use the existing procedural character and do not request the missing GLB. If an advertised asset fails to load, the game keeps the placeholder and shows a notice.

The supplied GLB has one textured mesh, three embedded 4096×4096 PNG maps, and no skeleton or animation clips. It is normalized to 1.9 m with its feet at ground level and parented to the existing player transform. Movement and facing work; walking/attacking skeletal animation requires a separately rigged/animated model. The practice opponent remains procedural.

Changing model geometry or replacing the download may require updating the scale/origin/facing normalization in crates/arena-client/src/character.rs. The original download has not been decimated, repainted, retextured, or resized by this integration.
