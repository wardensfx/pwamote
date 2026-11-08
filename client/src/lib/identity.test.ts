import { describe, expect, it } from "vitest";
import { guessDeviceName, loadIdentity, pairingCodeFromHash, parseScannedLink, saveIdentity, wsUrl } from "./identity";

const memory = () => {
  const m = new Map<string, string>();
  return { getItem: (k: string) => m.get(k) ?? null, setItem: (k: string, v: string) => void m.set(k, v), removeItem: (k: string) => void m.delete(k) };
};

describe("identity", () => {
  it("conserve et relit l'identité", () => {
    const s = memory();
    expect(loadIdentity(s)).toBeNull();
    saveIdentity({ token: "t", deviceId: "d" }, s);
    expect(loadIdentity(s)).toEqual({ token: "t", deviceId: "d" });
  });

  it("ignore un stockage corrompu", () => {
    const s = memory();
    s.setItem("pwamote.identity", "{oups");
    expect(loadIdentity(s)).toBeNull();
    s.setItem("pwamote.identity", '{"token":1}');
    expect(loadIdentity(s)).toBeNull();
  });

  it("lit le code dans le lien du QR", () => {
    expect(pairingCodeFromHash("#/pair/abcDEF123_-xyz")).toBe("abcDEF123_-xyz");
    expect(pairingCodeFromHash("#/pair/court")).toBeNull();
    expect(pairingCodeFromHash("#/autre/abcDEF123xyz")).toBeNull();
    expect(pairingCodeFromHash("")).toBeNull();
  });

  it("nomme l'appareil et construit l'URL WebSocket", () => {
    expect(guessDeviceName("Mozilla/5.0 (iPhone; CPU iPhone OS 18_0)")).toBe("iPhone");
    expect(wsUrl({ protocol: "https:", host: "pwamote-pc.local:8443" })).toBe("wss://pwamote-pc.local:8443/ws");
    expect(wsUrl({ protocol: "http:", host: "localhost:5173" })).toBe("ws://localhost:5173/ws");
  });
});

describe("parseScannedLink", () => {
  const here = "https://192.168.1.20:8443";
  it("reconnaît un lien d'appairage de cette adresse", () => {
    expect(parseScannedLink("https://192.168.1.20:8443/#/pair/abcdefgh12345678", here)).toEqual({ kind: "here", code: "abcdefgh12345678" });
  });
  it("signale un lien d'une autre adresse du PC", () => {
    const url = "https://pwamote-pc.local:8443/#/pair/abcdefgh12345678";
    expect(parseScannedLink(url, here)).toEqual({ kind: "elsewhere", url });
  });
  it("refuse tout le reste", () => {
    for (const t of ["bonjour", "javascript:alert(1)", "https://192.168.1.20:8443/", "https://x.test/#/pair/court", ""]) {
      expect(parseScannedLink(t, here)).toBeNull();
    }
  });
});
