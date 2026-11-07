import { defineConfig } from "vite";
export default defineConfig({
  root: "web",
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
  server: { host: "127.0.0.1", port: 8000 },
});
