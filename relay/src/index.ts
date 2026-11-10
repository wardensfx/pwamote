// Service DNS de PwaMote (Worker Cloudflare), sur le modèle de plex.direct.
//
// Chaque PC s'inscrit une fois et reçoit un nom `<id>.pwamote.app`. Il y publie
// son adresse **locale** (réseau privé uniquement) et le défi DNS-01 de
// Let's Encrypt ; le certificat lui-même est demandé par le PC, jamais ici.
// Le service ne voit donc ni clé privée, ni trafic : seulement des enregistrements DNS.

export interface Env {
  DEVICES: KVNamespace;
  /** Jeton d'API Cloudflare limité à la zone, droit DNS : Modifier (secret). */
  CF_API_TOKEN: string;
  ZONE_ID: string;
  ZONE_NAME: string;
}

type Device = { hash: string; created: number; seen: number };

const ID_ALPHABET = "abcdefghijklmnopqrstuvwxyz234567";
const ID_LENGTH = 12;
const REGISTER_PER_HOUR = 10;
const STALE_AFTER_DAYS = 180;
const CF_API = "https://api.cloudflare.com/client/v4";

// ------------------------------------------------------------- règles pures

/** Adresse IPv4 d'un réseau privé : 10/8, 172.16/12, 192.168/16, 100.64/10 (CGNAT, Tailscale). */
export function isPrivateIpv4(ip: string): boolean {
  const m = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/.exec(ip);
  if (!m) return false;
  const [a, b, c, d] = m.slice(1).map(Number);
  if ([a, b, c, d].some((n) => n > 255)) return false;
  return a === 10 || (a === 172 && b >= 16 && b <= 31) || (a === 192 && b === 168) || (a === 100 && b >= 64 && b <= 127);
}

/** Valeur d'un défi DNS-01 : SHA-256 en base64url, 43 caractères. */
export function isChallengeValue(v: string): boolean {
  return /^[A-Za-z0-9_-]{43}$/.test(v);
}

export function isDeviceId(id: string): boolean {
  return new RegExp(`^[${ID_ALPHABET}]{${ID_LENGTH}}$`).test(id);
}

function randomId(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(ID_LENGTH));
  return Array.from(bytes, (b) => ID_ALPHABET[b % 32]).join("");
}

function randomSecret(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(32));
  return btoa(String.fromCharCode(...bytes)).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

async function sha256(s: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(s));
  return Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, "0")).join("");
}

/** Comparaison à temps constant. */
function sameString(a: string, b: string): boolean {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= a.charCodeAt(i) ^ b.charCodeAt(i);
  return diff === 0;
}

// ------------------------------------------------------------ Cloudflare DNS

type DnsRecord = { id: string };

async function cf(env: Env, method: string, path: string, body?: unknown): Promise<unknown> {
  const rsp = await fetch(`${CF_API}/zones/${env.ZONE_ID}${path}`, {
    method,
    headers: { authorization: `Bearer ${env.CF_API_TOKEN}`, "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const json = (await rsp.json()) as { success: boolean; result: unknown; errors?: { message: string }[] };
  if (!json.success) throw new Error(`Cloudflare : ${(json.errors ?? []).map((e) => e.message).join(" ; ")}`);
  return json.result;
}

async function records(env: Env, type: string, name: string): Promise<DnsRecord[]> {
  return (await cf(env, "GET", `/dns_records?type=${type}&name=${encodeURIComponent(name)}`)) as DnsRecord[];
}

async function upsert(env: Env, type: string, name: string, content: string): Promise<void> {
  const body = { type, name, content, ttl: 60, proxied: false, comment: "pwamote" };
  const [existing, ...extra] = await records(env, type, name);
  if (existing) await cf(env, "PUT", `/dns_records/${existing.id}`, body);
  else await cf(env, "POST", "/dns_records", body);
  for (const r of extra) await cf(env, "DELETE", `/dns_records/${r.id}`);
}

async function removeAll(env: Env, type: string, name: string): Promise<void> {
  for (const r of await records(env, type, name)) await cf(env, "DELETE", `/dns_records/${r.id}`);
}

// ------------------------------------------------------------------ requêtes

const json = (data: unknown, status = 200) =>
  new Response(JSON.stringify(data), { status, headers: { "content-type": "application/json", "cache-control": "no-store" } });
const fail = (status: number, error: string) => json({ error }, status);

/** `Authorization: Bearer <id>.<secret>` → identifiant du PC, si le secret est le bon. */
async function authenticate(req: Request, env: Env): Promise<{ id: string; device: Device } | null> {
  const m = /^Bearer ([a-z2-7]+)\.([A-Za-z0-9_-]+)$/.exec(req.headers.get("authorization") ?? "");
  if (!m || !isDeviceId(m[1])) return null;
  const device = await env.DEVICES.get<Device>(`dev:${m[1]}`, "json");
  if (!device || !sameString(device.hash, await sha256(m[2]))) return null;
  return { id: m[1], device };
}

async function register(req: Request, env: Env): Promise<Response> {
  const client = req.headers.get("cf-connecting-ip") ?? "inconnu";
  const bucket = `rl:${client}:${Math.floor(Date.now() / 3_600_000)}`;
  const used = Number((await env.DEVICES.get(bucket)) ?? 0);
  if (used >= REGISTER_PER_HOUR) return fail(429, "trop d'inscriptions depuis cette adresse, réessayez dans une heure");
  await env.DEVICES.put(bucket, String(used + 1), { expirationTtl: 3600 });

  let id = randomId();
  while (await env.DEVICES.get(`dev:${id}`)) id = randomId();
  const secret = randomSecret();
  const now = Date.now();
  const device: Device = { hash: await sha256(secret), created: now, seen: now };
  await env.DEVICES.put(`dev:${id}`, JSON.stringify(device));
  return json({ id, secret, domain: `${id}.${env.ZONE_NAME}` }, 201);
}

async function body(req: Request): Promise<Record<string, unknown> | null> {
  try {
    const b = await req.json();
    return typeof b === "object" && b !== null ? (b as Record<string, unknown>) : null;
  } catch {
    return null;
  }
}

export async function handle(req: Request, env: Env): Promise<Response> {
  const { pathname } = new URL(req.url);
  if (pathname === "/v1/health") return json({ ok: true });
  if (pathname === "/v1/register" && req.method === "POST") return register(req, env);

  const auth = await authenticate(req, env);
  if (!auth) return fail(401, "identifiants invalides");
  const name = `${auth.id}.${env.ZONE_NAME}`;
  const touch = () => env.DEVICES.put(`dev:${auth.id}`, JSON.stringify({ ...auth.device, seen: Date.now() }));

  try {
    if (pathname === "/v1/address" && req.method === "PUT") {
      const ip = (await body(req))?.ip;
      if (typeof ip !== "string" || !isPrivateIpv4(ip)) return fail(400, "seules les adresses d'un réseau privé sont acceptées");
      await upsert(env, "A", name, ip);
      await touch();
      return json({ domain: name, ip });
    }
    if (pathname === "/v1/challenge" && req.method === "PUT") {
      const value = (await body(req))?.value;
      if (typeof value !== "string" || !isChallengeValue(value)) return fail(400, "valeur de défi invalide");
      await upsert(env, "TXT", `_acme-challenge.${name}`, `"${value}"`);
      await touch();
      return json({ ok: true });
    }
    if (pathname === "/v1/challenge" && req.method === "DELETE") {
      await removeAll(env, "TXT", `_acme-challenge.${name}`);
      return json({ ok: true });
    }
  } catch (e) {
    return fail(502, String(e instanceof Error ? e.message : e));
  }
  return fail(404, "inconnu");
}

/** Ménage : PC muets depuis 180 jours (leurs noms et leur inscription disparaissent). */
export async function sweep(env: Env, now = Date.now()): Promise<number> {
  let removed = 0;
  let cursor: string | undefined;
  do {
    const page = await env.DEVICES.list({ prefix: "dev:", cursor });
    for (const k of page.keys) {
      const d = await env.DEVICES.get<Device>(k.name, "json");
      if (d && now - d.seen > STALE_AFTER_DAYS * 86_400_000) {
        const name = `${k.name.slice(4)}.${env.ZONE_NAME}`;
        await removeAll(env, "A", name);
        await removeAll(env, "TXT", `_acme-challenge.${name}`);
        await env.DEVICES.delete(k.name);
        removed++;
      }
    }
    cursor = page.list_complete ? undefined : page.cursor;
  } while (cursor);
  return removed;
}

export default {
  fetch: (req: Request, env: Env) => handle(req, env),
  scheduled: async (_e: ScheduledController, env: Env, ctx: ExecutionContext) => ctx.waitUntil(sweep(env)),
} satisfies ExportedHandler<Env>;
