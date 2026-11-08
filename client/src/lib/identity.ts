export interface Identity {
  token: string;
  deviceId: string;
}

const KEY = "pwamote.identity";

export function loadIdentity(storage: Pick<Storage, "getItem"> = localStorage): Identity | null {
  try {
    const raw = storage.getItem(KEY);
    if (!raw) return null;
    const v = JSON.parse(raw) as Partial<Identity>;
    return typeof v.token === "string" && typeof v.deviceId === "string" ? { token: v.token, deviceId: v.deviceId } : null;
  } catch {
    return null;
  }
}

export function saveIdentity(id: Identity, storage: Pick<Storage, "setItem"> = localStorage): void {
  try {
    storage.setItem(KEY, JSON.stringify(id));
  } catch {
    /* navigation privée : on repassera par l'appairage */
  }
}

export function clearIdentity(storage: Pick<Storage, "removeItem"> = localStorage): void {
  try {
    storage.removeItem(KEY);
  } catch {
    /* rien */
  }
}

/** Extrait le code d'appairage d'un lien `…/#/pair/<code>`. */
export function pairingCodeFromHash(hash: string): string | null {
  const m = /^#\/pair\/([A-Za-z0-9_-]{8,128})$/.exec(hash);
  return m ? m[1]! : null;
}

/** Nom proposé pour ce téléphone dans la liste des appareils du PC. */
export function guessDeviceName(ua = navigator.userAgent): string {
  if (/iPhone/.test(ua)) return "iPhone";
  if (/iPad/.test(ua)) return "iPad";
  if (/Android/.test(ua)) return "Android";
  return "Téléphone";
}

export function wsUrl(loc: Pick<Location, "protocol" | "host">): string {
  return `${loc.protocol === "https:" ? "wss" : "ws"}://${loc.host}/ws`;
}

export type ScannedLink = { kind: "here"; code: string } | { kind: "elsewhere"; url: string };

/**
 * Interprète le contenu d'un QR code scanné dans l'app. Seuls les liens d'appairage `…/#/pair/<code>`
 * sont acceptés ; si le PC est joignable à une autre adresse que celle de cette page, il faut y aller.
 */
export function parseScannedLink(text: string, currentOrigin: string): ScannedLink | null {
  let url: URL;
  try {
    url = new URL(text.trim());
  } catch {
    return null;
  }
  if (url.protocol !== "https:" && url.protocol !== "http:") return null;
  const code = pairingCodeFromHash(url.hash);
  if (!code) return null;
  return url.origin === currentOrigin ? { kind: "here", code } : { kind: "elsewhere", url: url.href };
}
