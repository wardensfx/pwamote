import { defineConfig } from "vitest/config";
import solid from "vite-plugin-solid";
import { VitePWA } from "vite-plugin-pwa";
import { readFileSync } from "node:fs";

/**
 * L'icône de l'écran d'accueil iOS est téléchargée en dehors de Safari, qui ne fait pas confiance
 * à l'autorité de certification locale : le lien HTTPS échoue et l'app n'a pas d'icône.
 * On l'incorpore donc directement dans la page.
 */
const inlineTouchIcon = () => ({
  name: "inline-touch-icon",
  transformIndexHtml: (html: string) =>
    html.replace(
      '<link rel="apple-touch-icon" href="/icon-192.png" />',
      `<link rel="apple-touch-icon" href="data:image/png;base64,${readFileSync("public/icon-192.png").toString("base64")}" />`,
    ),
});

export default defineConfig({
  plugins: [
    solid(),
    inlineTouchIcon(),
    VitePWA({
      registerType: "autoUpdate",
      // L'enregistrement est fait dans main.tsx, qui vérifie aussi les mises à jour à la reprise.
      injectRegister: false,
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
