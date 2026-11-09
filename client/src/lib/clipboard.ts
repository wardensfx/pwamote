/**
 * Presse-papiers du téléphone vers le PC.
 *
 * Contraintes iOS : `navigator.clipboard.read()` n'est permis que dans un geste de
 * l'utilisateur (un tap), sur une page HTTPS, et Safari affiche alors une bulle « Coller »
 * à confirmer quand le contenu vient d'une autre app. Impossible donc de surveiller le
 * presse-papiers en tâche de fond : on propose l'envoi d'un tap, notamment au retour dans l'app.
 */

export interface PhoneClip {
  /** Type MIME envoyé tel quel au PC dans `Content-Type`. */
  type: string;
  data: Blob;
}

type ItemLike = { readonly types: readonly string[]; getType(type: string): Promise<Blob> };
type ClipboardLike = { read?: () => Promise<readonly ItemLike[]>; readText: () => Promise<string> };
type FetchLike = (url: string, init: RequestInit) => Promise<Response>;

/** Formats que le PC sait décoder directement. */
const DIRECT_IMAGES = ["image/png", "image/jpeg", "image/gif", "image/webp", "image/bmp"];
const TEXT = "text/plain;charset=utf-8";

/** Ré-encode une image (HEIC, TIFF…) en PNG via un canvas. */
export async function convertToPng(blob: Blob): Promise<Blob> {
  const bitmap = await createImageBitmap(blob);
  const canvas = document.createElement("canvas");
  canvas.width = bitmap.width;
  canvas.height = bitmap.height;
  canvas.getContext("2d")!.drawImage(bitmap, 0, 0);
  bitmap.close();
  return new Promise((resolve, reject) =>
    canvas.toBlob((b) => (b ? resolve(b) : reject(new Error("Image illisible sur le téléphone."))), "image/png"),
  );
}

/** Choisit ce qu'on envoie parmi les éléments du presse-papiers : l'image d'abord, sinon le texte. */
export async function pickClip(items: readonly ItemLike[], toPng: (b: Blob) => Promise<Blob> = convertToPng): Promise<PhoneClip | null> {
  for (const it of items) {
    const type = it.types.find((t) => DIRECT_IMAGES.includes(t));
    if (type) return { type, data: await it.getType(type) };
  }
  for (const it of items) {
    const type = it.types.find((t) => t.startsWith("image/"));
    if (type) return { type: "image/png", data: await toPng(await it.getType(type)) };
  }
  for (const it of items) {
    if (!it.types.includes("text/plain")) continue;
    const text = await (await it.getType("text/plain")).text();
    if (text) return { type: TEXT, data: new Blob([text], { type: TEXT }) };
  }
  return null;
}

/**
 * Lit le presse-papiers du téléphone. À appeler directement dans le gestionnaire du tap :
 * `read()` est lancé avant tout `await`, sinon iOS refuse.
 */
export async function readPhoneClipboard(clip: ClipboardLike = navigator.clipboard as unknown as ClipboardLike): Promise<PhoneClip | null> {
  try {
    if (clip.read) return await pickClip(await clip.read());
    const text = await clip.readText();
    return text ? { type: TEXT, data: new Blob([text], { type: TEXT }) } : null;
  } catch (e) {
    if (e instanceof Error && e.name === "NotAllowedError") {
      throw new Error("Accès au presse-papiers refusé : touchez « Coller » dans la bulle de Safari.");
    }
    throw e instanceof Error ? e : new Error(String(e));
  }
}

/** Image collée dans un champ (appui long › Coller) : pas de bulle de confirmation. */
export function clipFromDataTransfer(dt: Pick<DataTransfer, "files" | "getData">): PhoneClip | null {
  for (const f of Array.from(dt.files)) {
    if (f.type.startsWith("image/")) return { type: f.type, data: f };
  }
  return null;
}

/** Envoie le contenu au PC, qui le place dans son presse-papiers. Lève une erreur lisible. */
export async function sendClip(clip: PhoneClip, token: string, fetchFn: FetchLike = fetch): Promise<void> {
  let res: Response;
  try {
    res = await fetchFn("/clipboard", {
      method: "POST",
      headers: { "Content-Type": clip.type, Authorization: `Bearer ${token}` },
      body: clip.data,
    });
  } catch {
    throw new Error("Le PC ne répond pas.");
  }
  if (res.ok) return;
  let message: string | undefined;
  try {
    message = ((await res.json()) as { message?: string }).message;
  } catch {
    /* corps non JSON : message par défaut ci-dessous */
  }
  if (message) throw new Error(message);
  if (res.status === 413) throw new Error("Contenu trop volumineux pour le PC.");
  throw new Error(`Le PC a refusé le presse-papiers (${res.status}).`);
}

/** Absence minimale hors de l'app pour proposer l'envoi au retour (on est sans doute allé copier). */
const AWAY_MIN_MS = 1000;

export function shouldOfferOnReturn(enabled: boolean, hiddenAt: number, now: number): boolean {
  return enabled && now - hiddenAt >= AWAY_MIN_MS;
}
