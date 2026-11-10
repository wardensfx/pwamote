import type { AppConfig } from "../../client/src/generated/AppConfig";
import type { Candidate } from "../../client/src/generated/Candidate";
import type { PublicCertStatus } from "../../client/src/generated/PublicCertStatus";

/**
 * Valeur du menu d'adresses pour la configuration courante :
 * "auto", "mdns", "ip:<adresse>" ou "custom" (saisie libre).
 */
export function selectedAddress(config: AppConfig, candidates: Candidate[]): string {
  const a = config.address;
  switch (a.kind) {
    case "auto":
      return "auto";
    case "mdns":
      return "mdns";
    case "ip":
      return candidates.some((c) => c.host === a.ip) ? `ip:${a.ip}` : "custom";
    case "custom":
      return "custom";
  }
}

const IPV4 = /^(\d{1,3})(\.\d{1,3}){3}$/;

/** Même règle que le serveur : nom DNS simple ou adresse IPv4. */
export function isValidHost(host: string): boolean {
  const h = host.trim();
  if (h === "" || h.length > 253) return false;
  if (IPV4.test(h)) return h.split(".").every((n) => Number(n) <= 255);
  return h.split(".").every((l) => l.length > 0 && l.length <= 63 && !l.startsWith("-") && !l.endsWith("-") && /^[A-Za-z0-9-]+$/.test(l));
}

export function relativeTime(now: number, then: number): string {
  if (!then) return "jamais";
  const s = Math.max(0, now - then);
  if (s < 60) return "à l'instant";
  if (s < 3600) return `il y a ${Math.floor(s / 60)} min`;
  if (s < 86400) return `il y a ${Math.floor(s / 3600)} h`;
  return `il y a ${Math.floor(s / 86400)} j`;
}

export function countdown(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

export function phonesLabel(n: number): string {
  if (n === 0) return "Aucun téléphone connecté";
  return n === 1 ? "1 téléphone connecté" : `${n} téléphones connectés`;
}


export type CertTone = "ok" | "wait" | "error" | "off";

/** Résumé du certificat Let's Encrypt pour la fenêtre : pastille, titre, détail. */
export function publicCertSummary(s: PublicCertStatus, now: number): { tone: CertTone; title: string; detail: string } {
  const test = s.staging ? " (serveur de test)" : "";
  switch (s.phase) {
    case "disabled":
      return { tone: "off", title: "Certificat local uniquement", detail: "Les iPhone doivent installer le profil de confiance PwaMote." };
    case "incomplete":
      return { tone: "error", title: "Réglages incomplets", detail: s.message ?? "" };
    case "waiting_for_network":
      return { tone: "wait", title: "En attente d'internet", detail: s.message ?? "Le certificat local sert en attendant." };
    case "obtaining":
      return { tone: "wait", title: `Demande à Let's Encrypt en cours${test}…`, detail: `${s.domain ?? ""} · une à deux minutes.` };
    case "failed":
      return { tone: "error", title: "Certificat non obtenu", detail: `${s.message ?? ""}${retryIn(s, now)}` };
    case "active": {
      const until = s.expires_at ? `valide encore ${daysLeft(now, s.expires_at)}` : "valide";
      const name = s.domain ? `${s.domain} · ` : "";
      return { tone: s.message ? "wait" : "ok", title: `Let's Encrypt${test} · ${until}`, detail: s.message ?? `${name}renouvelé automatiquement 30 jours avant l'échéance.` };
    }
  }
}

function daysLeft(now: number, at: number): string {
  const d = Math.floor((at - now) / 86400);
  return d >= 1 ? `${d} j` : "moins d'un jour";
}

function retryIn(s: PublicCertStatus, now: number): string {
  if (!s.next_check_at) return "";
  const min = Math.max(1, Math.round((s.next_check_at - now) / 60));
  return min < 120 ? ` Nouvel essai dans ${min} min.` : ` Nouvel essai dans ${Math.round(min / 60)} h.`;
}
