import { defineConfig } from "vitest/config";
import solid from "vite-plugin-solid";

export default defineConfig({
  plugins: [solid()],
  base: "./",
  build: { outDir: "dist", target: "es2022" },
  test: { environment: "jsdom", include: ["src/**/*.test.ts"] },
});
