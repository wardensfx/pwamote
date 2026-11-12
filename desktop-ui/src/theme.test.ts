import { describe, expect, it } from "vitest";
import confRaw from "../../crates/pwamote-desktop/tauri.conf.json?raw";
import html from "../index.html?raw";
import css from "./styles.css?raw";

// L'interface est conçue pour un fond sombre : elle doit le rester quel que soit
// le thème de Windows (clair) ou l'état de Mica (fenêtre inactive, transparence coupée).
const conf = JSON.parse(confRaw);

describe("thème sombre forcé", () => {
  it("la fenêtre impose le thème sombre (barre de titre, Mica, WebView2)", () => {
    const main = conf.app.windows.find((w: { label: string }) => w.label === "main");
    expect(main.theme).toBe("Dark");
  });
  it("la page déclare color-scheme: dark avant même le chargement du CSS", () => {
    expect(html).toMatch(/<meta name="color-scheme" content="dark"/);
    expect(css).toMatch(/:root\s*{[^}]*color-scheme:\s*dark/);
  });
  it("avec Mica, le fond garde un voile sombre au lieu d'être totalement transparent", () => {
    const rule = css.match(/body:has\(main\.backdrop\)\s*{([^}]*)}/)?.[1] ?? "";
    expect(rule).not.toMatch(/background:\s*transparent/);
    expect(rule).toMatch(/background:\s*rgba\(/);
  });
});
