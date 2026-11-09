/**
 * Collage d'une image depuis le champ texte du téléphone vers le PC.
 *
 * Un texte collé dans le champ est simplement tapé sur le PC. Une image, elle, ne peut pas
 * entrer dans un `<input>` : on l'intercepte au collage (`clipboardData`, sans bulle « Coller »
 * d'iOS), on l'envoie au PC qui la met dans son presse-papiers et fait Ctrl+V.
 */

export interface PhoneClip {
  /** Type MIME envoyé tel quel au PC dans `Content-Type`. */
  type: string;
  data: Blob;
}

type FetchLike = (url: string, init: RequestInit) => Promise<Response>;

/** Formats que le PC sait décoder directement. */
const DIRECT_IMAGES = ["image/png", "image/jpeg", "image/gif", "image/webp", "image/bmp"];

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

/** Image collée dans le champ (appui long › Coller). Null pour un collage de texte seul. */
export function clipFromDataTransfer(dt: Pick<DataTransfer, "files" | "getData">): PhoneClip | null {
  for (const f of Array.from(dt.files)) {
    if (f.type.startsWith("image/")) return { type: f.type, data: f };
  }
  return null;
}

/** Les formats que le PC ne décode pas (HEIC, TIFF…) sont ré-encodés en PNG sur le téléphone. */
export async function toPcFormat(clip: PhoneClip, toPng: (b: Blob) => Promise<Blob> = convertToPng): Promise<PhoneClip> {
  if (!clip.type.startsWith("image/") || DIRECT_IMAGES.includes(clip.type)) return clip;
  return { type: "image/png", data: await toPng(clip.data) };
}

/** Envoie le contenu au PC, qui le colle (presse-papiers puis Ctrl+V). Lève une erreur lisible. */
export async function sendClip(clip: PhoneClip, token: string, fetchFn: FetchLike = fetch): Promise<void> {
  let res: Response;
  try {
    res = await fetchFn("/paste", {
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
  throw new Error(`Le PC a refusé le collage (${res.status}).`);
}
