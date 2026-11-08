import { defineConfig } from "vitest/config";
import solid from "vite-plugin-solid";
import { VitePWA } from "vite-plugin-pwa";

export default defineConfig({
  plugins: [
    solid(),
    VitePWA({
      registerType: "autoUpdate",
      manifest: {
        name: "PwaMote",
        short_name: "PwaMote",
        description: "Votre téléphone, télécommande de votre PC.",
        display: "standalone",
        orientation: "portrait",
        background_color: "#0b0c0f",
        theme_color: "#0b0c0f",
        start_url: "/",
        icons: [
          { src: "icon.svg", sizes: "any", type: "image/svg+xml", purpose: "any" },
          { src: "icon-192.png", sizes: "192x192", type: "image/png" },
          { src: "icon-512.png", sizes: "512x512", type: "image/png", purpose: "any maskable" },
        ],
      },
      workbox: { navigateFallbackDenylist: [/^\/ws/, /^\/ca\.cer/, /^\/pwamote\.mobileconfig/] },
    }),
  ],
  // `dist` est embarqué dans le serveur Rust (crates/pwamote-server/assets).
  build: { outDir: "dist", target: "es2022" },
  server: { host: true, proxy: { "/ws": { target: "ws://localhost:8443", ws: true, secure: false } } },
  test: { environment: "jsdom", include: ["src/**/*.test.ts", "src/**/*.test.tsx"] },
});
