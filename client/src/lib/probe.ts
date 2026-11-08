/** Ce que l'on sait du PC quand la connexion ne passe pas. */
export type Reachability = "checking" | "reachable" | "unreachable";

/**
 * Interroge `/healthz` sans cache, pour distinguer « le PC ne répond pas à cette adresse » (réseau,
 * adresse changée, pare-feu, serveur arrêté) de « le PC répond mais la session échoue ».
 */
export async function probeServer(
  fetchFn: (url: string, init: RequestInit) => Promise<{ ok: boolean }> = (u, i) => fetch(u, i),
  timeoutMs = 4000,
  now: () => number = Date.now,
): Promise<Exclude<Reachability, "checking">> {
  const ctrl = typeof AbortController === "undefined" ? null : new AbortController();
  const timer = setTimeout(() => ctrl?.abort(), timeoutMs);
  try {
    const res = await fetchFn(`/healthz?t=${now()}`, { cache: "no-store", signal: ctrl?.signal });
    return res.ok ? "reachable" : "unreachable";
  } catch {
    return "unreachable";
  } finally {
    clearTimeout(timer);
  }
}
