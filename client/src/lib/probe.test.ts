import { describe, expect, it } from "vitest";
import { probeServer } from "./probe";

describe("sonde du PC", () => {
  it("dit joignable quand /healthz répond, sans cache", async () => {
    let seen: [string, RequestInit] | null = null;
    const r = await probeServer(async (u, i) => ((seen = [u, i]), { ok: true }), 1000, () => 42);
    expect(r).toBe("reachable");
    expect(seen![0]).toBe("/healthz?t=42");
    expect(seen![1].cache).toBe("no-store");
  });

  it("dit injoignable sur une erreur réseau, une réponse en erreur ou un délai dépassé", async () => {
    expect(await probeServer(async () => Promise.reject(new TypeError("Load failed")))).toBe("unreachable");
    expect(await probeServer(async () => ({ ok: false }))).toBe("unreachable");
    const hang = (_u: string, i: RequestInit) =>
      new Promise<{ ok: boolean }>((_, reject) => i.signal?.addEventListener("abort", () => reject(new Error("abort"))));
    expect(await probeServer(hang, 20)).toBe("unreachable");
  });
});
