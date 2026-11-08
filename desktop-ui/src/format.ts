import type { AppConfig } from "../../client/src/generated/AppConfig";
import type { Candidate } from "../../client/src/generated/Candidate";

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
