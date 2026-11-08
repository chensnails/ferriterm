import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
  },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  build: {
    target: "es2021",
    minify: "esbuild",
    sourcemap: false,
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (!id.includes("node_modules")) return;
          if (id.includes("/@xterm/")) return "xterm";
          if (id.includes("/@tauri-apps/")) return "tauri";
          if (
            id.includes("/react/") ||
            id.includes("/react-dom/") ||
            id.includes("/scheduler/")
          ) {
            return "react";
          }
          if (
            id.includes("/i18next/") ||
            id.includes("/i18next-icu/") ||
            id.includes("/react-i18next/") ||
            id.includes("/intl-messageformat/") ||
            id.includes("/@formatjs/")
          ) {
            return "i18n";
          }
          return "vendor";
        },
      },
    },
  },
});