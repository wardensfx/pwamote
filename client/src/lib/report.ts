// « Signaler un problème » : règles partagées par la PWA et l'application de bureau.
// Le canal (support PwaMote ou issue GitHub) est décidé par le PC, voir report.rs.

import type { ReportInfo } from "../generated/ReportInfo";
import type { ReportRequest } from "../generated/ReportRequest";

/** Comme le serveur (report.rs) et le support. */
export const MAX_MESSAGE = 5000;

/**
 * Système du téléphone, sans modèle ni navigateur : « iOS 18.1 », « Android 15 ».
 * `touchPoints` : un iPad récent se présente comme un Mac, seul l'écran tactile le trahit.
 */
export function phoneOs(ua: string, touchPoints = 0): string {
  const ios = /\b(iPhone|iPad|iPod)\b.*? OS (\d+(?:_\d+)*)/.exec(ua);
  if (ios) return `${ios[1] === "iPad" ? "iPadOS" : "iOS"} ${ios[2]!.replace(/_/g, ".")}`;
  const android = /\bAndroid (\d+(?:\.\d+)*)/.exec(ua);
  if (android) return `Android ${android[1]}`;
  if (/\bMacintosh\b/.test(ua)) return touchPoints > 1 ? "iPadOS" : "macOS";
  if (/\bWindows\b/.test(ua)) return "Windows";
  if (/\bLinux\b/.test(ua)) return "Linux";
  return "";
}

export function emailOk(email: string): boolean {
  const e = email.trim();
  return e === "" || (e.length <= 200 && /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(e));
}

export function canSend(message: string, email: string): boolean {
  const m = message.trim();
  return m.length > 0 && m.length <= MAX_MESSAGE && emailOk(email);
}

/** Ajoute le système du téléphone au lien de l'issue préremplie (champ `phone` du modèle). */
export function withPhone(issueUrl: string, phone: string): string {
  if (!phone) return issueUrl;
  const u = new URL(issueUrl);
  u.searchParams.set("phone", phone);
  return u.toString();
}

/** Langue de la réponse du support : français pour un navigateur en français, anglais sinon. */
export function reportLang(navigatorLang: string | undefined): "fr" | "en" {
  if (!navigatorLang) return "fr";
  return navigatorLang.toLowerCase().startsWith("fr") ? "fr" : "en";
}

// ------------------------------------------------------------------ échanges avec le PC


type FetchLike = (url: string, init?: RequestInit) => Promise<Response>;

async function call(fetchFn: FetchLike, url: string, init: RequestInit): Promise<Response> {
  try {
    return await fetchFn(url, init);
  } catch {
    throw new Error("Le PC ne répond pas.");
  }
}

async function failure(res: Response, fallback: string): Promise<Error> {
  try {
    const m = ((await res.json()) as { message?: string }).message;
    if (m) return new Error(m);
  } catch {
    /* corps non JSON */
  }
  return new Error(fallback);
}

/** Canal du PC (support PwaMote ou GitHub) et lien de l'issue préremplie. */
export async function fetchReportInfo(token: string, fetchFn: FetchLike = fetch): Promise<ReportInfo> {
  const res = await call(fetchFn, "/report", { headers: { Authorization: `Bearer ${token}` } });
  if (!res.ok) throw await failure(res, res.status === 404 ? "Ce PC ne propose pas encore le signalement : mettez PwaMote à jour sur le PC." : `Le PC a refusé (${res.status}).`);
  return (await res.json()) as ReportInfo;
}

/** Envoie le signalement au support PwaMote via le PC ; renvoie la référence du ticket. */
export async function sendReport(token: string, req: ReportRequest, fetchFn: FetchLike = fetch): Promise<string> {
  const res = await call(fetchFn, "/report", {
    method: "POST",
    headers: { "Content-Type": "application/json", Authorization: `Bearer ${token}` },
    body: JSON.stringify(req),
  });
  if (!res.ok) throw await failure(res, `Envoi impossible (${res.status}).`);
  return ((await res.json()) as { ticket: string }).ticket;
}
