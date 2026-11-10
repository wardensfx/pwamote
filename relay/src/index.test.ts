import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { handle, isChallengeValue, isPrivateIpv4, sweep, type Env } from "./index";

/** KV en mémoire. */
function memoryKv() {
  const m = new Map<string, string>();
  return {
    map: m,
    get: async (k: string, type?: string) => {
      const v = m.get(k);
      return v === undefined ? null : type === "json" ? JSON.parse(v) : v;
    },
    put: async (k: string, v: string) => void m.set(k, v),
    delete: async (k: string) => void m.delete(k),
    list: async ({ prefix }: { prefix: string }) => ({ keys: [...m.keys()].filter((k) => k.startsWith(prefix)).map((name) => ({ name })), list_complete: true }),
  };
}

/** API DNS de Cloudflare en mémoire. */
type Rec = { id: string; type: string; name: string; content: string };
function fakeCloudflare() {
  const recs: Rec[] = [];
  let n = 0;
  const fetch = vi.fn(async (input: string, init?: RequestInit) => {
    const url = new URL(input);
    const ok = (result: unknown) => new Response(JSON.stringify({ success: true, result }));
    expect(new Headers(init?.headers).get("authorization")).toBe("Bearer cf-token");
    if (url.pathname === "/client/v4/zones") return ok([{ id: "zone1" }]);
    const path = url.pathname.replace("/client/v4/zones/zone1", "");
    const method = init?.method ?? "GET";
    if (path === "/dns_records" && method === "GET")
      return ok(recs.filter((r) => r.type === url.searchParams.get("type") && r.name === url.searchParams.get("name")));
    if (path === "/dns_records" && method === "POST") {
      const r = { ...(JSON.parse(String(init!.body)) as Rec), id: `r${++n}` };
      recs.push(r);
      return ok(r);
    }
    const id = path.split("/")[2];
    const i = recs.findIndex((r) => r.id === id);
    if (method === "PUT") recs[i] = { ...(JSON.parse(String(init!.body)) as Rec), id };
    if (method === "DELETE") recs.splice(i, 1);
    return ok({ id });
  });
  return { recs, fetch };
}

let kv: ReturnType<typeof memoryKv>;
let cf: ReturnType<typeof fakeCloudflare>;
let env: Env;

beforeEach(() => {
  kv = memoryKv();
  cf = fakeCloudflare();
  vi.stubGlobal("fetch", cf.fetch);
  env = { DEVICES: kv as unknown as KVNamespace, CF_API_TOKEN: "cf-token", ZONE_NAME: "pwamote.app" };
});
afterEach(() => vi.unstubAllGlobals());

const call = (method: string, path: string, opts: { auth?: string; body?: unknown; ip?: string } = {}) =>
  handle(
    new Request(`https://dns.pwamote.app${path}`, {
      method,
      headers: {
        ...(opts.auth ? { authorization: `Bearer ${opts.auth}` } : {}),
        "cf-connecting-ip": opts.ip ?? "203.0.113.7",
      },
      body: opts.body === undefined ? undefined : JSON.stringify(opts.body),
    }),
    env,
  );

async function registered() {
  const rsp = await call("POST", "/v1/register");
  expect(rsp.status).toBe(201);
  const r = (await rsp.json()) as { id: string; secret: string; domain: string };
  return { ...r, auth: `${r.id}.${r.secret}` };
}

const VALUE = "a".repeat(43);

describe("règles", () => {
  it("adresses privées uniquement", () => {
    for (const ok of ["10.0.0.1", "172.16.5.4", "172.31.255.255", "192.168.1.20", "100.64.0.1"]) expect(isPrivateIpv4(ok), ok).toBe(true);
    for (const bad of ["8.8.8.8", "172.32.0.1", "192.169.0.1", "127.0.0.1", "1.2.3", "256.1.1.1", "::1", "192.168.1.20 "]) expect(isPrivateIpv4(bad), bad).toBe(false);
  });
  it("valeur de défi : 43 caractères base64url", () => {
    expect(isChallengeValue("abc_DEF-123".padEnd(43, "x"))).toBe(true);
    expect(isChallengeValue("x".repeat(42))).toBe(false);
    expect(isChallengeValue('"x'.padEnd(43, "x"))).toBe(false);
  });
});

describe("inscription", () => {
  it("donne un nom unique et un secret, conservé seulement sous forme d'empreinte", async () => {
    const a = await registered();
    const b = await registered();
    expect(a.domain).toMatch(/^[a-z2-7]{12}\.pwamote\.app$/);
    expect(a.id).not.toBe(b.id);
    const stored = kv.map.get(`dev:${a.id}`)!;
    expect(stored).not.toContain(a.secret);
  });
  it("limite les inscriptions par adresse", async () => {
    for (let i = 0; i < 10; i++) expect((await call("POST", "/v1/register", { ip: "198.51.100.1" })).status).toBe(201);
    expect((await call("POST", "/v1/register", { ip: "198.51.100.1" })).status).toBe(429);
    expect((await call("POST", "/v1/register", { ip: "198.51.100.2" })).status).toBe(201);
  });
});

describe("adresse et défi", () => {
  it("publie l'adresse locale, la met à jour sans doublon", async () => {
    const d = await registered();
    expect((await call("PUT", "/v1/address", { auth: d.auth, body: { ip: "192.168.1.20" } })).status).toBe(200);
    expect((await call("PUT", "/v1/address", { auth: d.auth, body: { ip: "192.168.1.42" } })).status).toBe(200);
    expect(cf.recs).toEqual([expect.objectContaining({ type: "A", name: d.domain, content: "192.168.1.42", proxied: false })]);
  });
  it("refuse une adresse publique (pas de nom pwamote.app vers internet)", async () => {
    const d = await registered();
    expect((await call("PUT", "/v1/address", { auth: d.auth, body: { ip: "8.8.8.8" } })).status).toBe(400);
    expect(cf.recs).toHaveLength(0);
  });
  it("publie puis retire le défi DNS-01", async () => {
    const d = await registered();
    expect((await call("PUT", "/v1/challenge", { auth: d.auth, body: { value: VALUE } })).status).toBe(200);
    expect(cf.recs).toEqual([expect.objectContaining({ type: "TXT", name: `_acme-challenge.${d.domain}`, content: `"${VALUE}"` })]);
    expect((await call("DELETE", "/v1/challenge", { auth: d.auth })).status).toBe(200);
    expect(cf.recs).toHaveLength(0);
  });
  it("un PC ne peut toucher qu'à son propre nom", async () => {
    const a = await registered();
    const b = await registered();
    expect((await call("PUT", "/v1/address", { auth: `${a.id}.${b.secret}`, body: { ip: "10.0.0.1" } })).status).toBe(401);
    expect((await call("PUT", "/v1/address", { body: { ip: "10.0.0.1" } })).status).toBe(401);
    expect((await call("PUT", "/v1/address", { auth: "../x.y", body: { ip: "10.0.0.1" } })).status).toBe(401);
    expect(cf.recs).toHaveLength(0);
  });
});

describe("ménage", () => {
  it("supprime les PC muets depuis 180 jours et leurs enregistrements", async () => {
    const old = await registered();
    const fresh = await registered();
    await call("PUT", "/v1/address", { auth: old.auth, body: { ip: "10.0.0.1" } });
    await call("PUT", "/v1/address", { auth: fresh.auth, body: { ip: "10.0.0.2" } });
    const later = Date.now() + 181 * 86_400_000;
    const d = JSON.parse(kv.map.get(`dev:${fresh.id}`)!);
    kv.map.set(`dev:${fresh.id}`, JSON.stringify({ ...d, seen: later - 86_400_000 }));
    expect(await sweep(env, later)).toBe(1);
    expect(kv.map.has(`dev:${old.id}`)).toBe(false);
    expect(cf.recs.map((r) => r.content)).toEqual(["10.0.0.2"]);
  });
});
