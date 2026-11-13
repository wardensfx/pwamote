import { describe, expect, it } from "vitest";
import type { UpdateInfo } from "../../client/src/generated/UpdateInfo";
import { licenceHint, licenceLabel, updateLabel, versionHint, versionText } from "./version";

const info = (over: Partial<UpdateInfo> = {}): UpdateInfo => ({ current: "1.2.0", official: true, available: null, ...over });
const release = { version: "1.3.0", url: "https://github.com/wardensfx/pwamote/releases/tag/v1.3.0" };

describe("versionText", () => {
  it("préfixe la version d'un v", () => {
    expect(versionText(info())).toBe("v1.2.0");
  });
  it("signale une compilation locale", () => {
    expect(versionText(info({ current: "0.1.0", official: false }))).toBe("v0.1.0 · dev");
  });
});

describe("versionHint", () => {
  it("explique qu'une compilation locale ne vérifie rien", () => {
    expect(versionHint(info({ official: false }))).toMatch(/compilée localement/i);
  });
  it("indique qu'une version officielle est à jour", () => {
    expect(versionHint(info())).toMatch(/à jour/);
  });
  it("nomme la nouvelle version", () => {
    expect(versionHint(info({ available: release }))).toContain("1.3.0");
  });
});

describe("updateLabel", () => {
  it("rien sans mise à jour", () => {
    expect(updateLabel(info())).toBeNull();
  });
  it("nomme la version disponible", () => {
    expect(updateLabel(info({ available: release }))).toBe("Mise à jour 1.3.0");
  });
  it("jamais pour une compilation locale", () => {
    expect(updateLabel(info({ official: false, available: release }))).toBeNull();
  });
});

describe("licenceLabel", () => {
  it("nom seul sans licence ou sans échéance", () => {
    expect(licenceLabel({ plan: "Casual", licensed: false, expires: null })).toBe("Casual");
    expect(licenceLabel({ plan: "Plus", licensed: true, expires: null })).toBe("Plus");
  });
  it("ajoute l'échéance d'une licence", () => {
    expect(licenceLabel({ plan: "Plus", licensed: true, expires: Date.UTC(2026, 11, 12, 12) })).toBe("Plus · jusqu'au 12/12");
  });
  it("info-bulle selon la licence", () => {
    expect(licenceHint({ plan: "Plus", licensed: true, expires: null })).toMatch(/active/);
    expect(licenceHint({ plan: "Casual", licensed: false, expires: null })).toMatch(/aucune licence/);
  });
});
