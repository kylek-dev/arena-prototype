import { defineConfig } from "vite";
import { mapEditorServer } from "./scripts/map-editor-server.mjs";
export default defineConfig({
  root: "web",
  // Local-only map loading and saving for the in-game editor (dev/preview).
  plugins: [mapEditorServer()],
  base: "./",
  publicDir: "public",
  build: {
    outDir: "../dist",
    emptyOutDir: true,
    rollupOptions: {
      onwarn(warning, warn) {
        // This application is intentionally client-only. Radix publishes the
        // same entry points for RSC consumers; their directives are irrelevant
        // here. Keep every other build warning visible.
        if (
          warning.code === "MODULE_LEVEL_DIRECTIVE" &&
          warning.message.includes('"use client"') &&
          warning.id?.includes("/node_modules/")
        )
          return;
        warn(warning);
      },
    },
  },
  server: {
    host: "127.0.0.1",
    port: 8000,
    // The front door (`scripts/dev-door.sh`) on the page's own origin, as a
    // deployment's reverse proxy does: match links, the lobby and the queue.
    // Preview uses the same proxy.
    proxy: { "/api": { target: "http://127.0.0.1:8090", ws: true } },
  },
});
