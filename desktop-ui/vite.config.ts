import { defineConfig } from "vitest/config";
import solid from "vite-plugin-solid";

export default defineConfig({
  plugins: [solid()],
  base: "./",
  // Les tests lisent aussi la configuration Tauri voisine (thème de la fenêtre).
  server: { fs: { allow: [".."] } },
  build: { outDir: "dist", target: "es2022" },
  test: { environment: "jsdom", include: ["src/**/*.test.ts"], css: { include: [/styles\.css/] } },
});
