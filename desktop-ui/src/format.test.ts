import { describe, expect, it } from "vitest";
import type { AppConfig } from "../../client/src/generated/AppConfig";
import type { Candidate } from "../../client/src/generated/Candidate";
import { countdown, isValidHost, phonesLabel, relativeTime, selectedAddress } from "./format";

const candidates: Candidate[] = [
  { choice: { kind: "auto" }, host: "192.168.1.20", label: "192.168.1.20", recommended: true },
  { choice: { kind: "ip", ip: "10.0.0.4" }, host: "10.0.0.4", label: "10.0.0.4", recommended: false },
  { choice: { kind: "mdns" }, host: "pwamote-pc.local", label: "pwamote-pc.local", recommended: false },
];
const cfg = (address: AppConfig["address"]): AppConfig => ({
  port: 8443, server_enabled: true, address, launch_at_login: false, start_minimized: false, close_to_tray: true,
});

describe("selectedAddress", () => {
  it("auto correspond à la proposition recommandée", () => {
    expect(selectedAddress(cfg({ kind: "auto" }), candidates)).toBe("auto");
  });
  it("une IP précise, le nom mDNS et une adresse saisie ont chacun leur valeur", () => {
    expect(selectedAddress(cfg({ kind: "ip", ip: "10.0.0.4" }), candidates)).toBe("ip:10.0.0.4");
    expect(selectedAddress(cfg({ kind: "mdns" }), candidates)).toBe("mdns");
    expect(selectedAddress(cfg({ kind: "custom", host: "pc.maison" }), candidates)).toBe("custom");
  });
  it("une IP qui n'existe plus sur la machine passe en saisie libre", () => {
    expect(selectedAddress(cfg({ kind: "ip", ip: "172.16.0.9" }), candidates)).toBe("custom");
  });
});

describe("isValidHost", () => {
  it("accepte noms et adresses", () => {
    for (const h of ["pc", "pc.maison", "192.168.0.2", "a-b.c1"]) expect(isValidHost(h), h).toBe(true);
  });
  it("refuse le reste", () => {
    for (const h of ["", " ", "-a", "a..b", "a b", "http://x", "a/b", "é.fr"]) expect(isValidHost(h), h).toBe(false);
  });
});

describe("relativeTime", () => {
  const now = 1_000_000;
  it("formule en français", () => {
    expect(relativeTime(now, now - 5)).toBe("à l'instant");
    expect(relativeTime(now, now - 120)).toBe("il y a 2 min");
    expect(relativeTime(now, now - 3 * 3600)).toBe("il y a 3 h");
    expect(relativeTime(now, now - 2 * 86400)).toBe("il y a 2 j");
  });
  it("jamais connecté", () => expect(relativeTime(now, 0)).toBe("jamais"));
});

describe("countdown et pluriels", () => {
  it("mm:ss", () => {
    expect(countdown(272)).toBe("4:32");
    expect(countdown(5)).toBe("0:05");
    expect(countdown(-3)).toBe("0:00");
  });
  it("téléphones connectés", () => {
    expect(phonesLabel(0)).toBe("Aucun téléphone connecté");
    expect(phonesLabel(1)).toBe("1 téléphone connecté");
    expect(phonesLabel(3)).toBe("3 téléphones connectés");
  });
});
