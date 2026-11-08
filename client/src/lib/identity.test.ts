import { describe, expect, it } from "vitest";
import { guessDeviceName, loadIdentity, pairingCodeFromHash, saveIdentity, wsUrl } from "./identity";

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
