import { describe, expect, it, vi } from "vitest";
import { clipFromDataTransfer, pickClip, readPhoneClipboard, sendClip, shouldOfferOnReturn } from "./clipboard";

const item = (entries: Record<string, string>) => ({
  types: Object.keys(entries),
  getType: async (t: string) => new Blob([entries[t]!], { type: t }),
});

describe("pickClip", () => {
  it("préfère l'image au texte quand le presse-papiers contient les deux", async () => {
    const clip = await pickClip([item({ "text/plain": "légende", "image/png": "PNG" })]);
    expect(clip?.type).toBe("image/png");
    expect(await clip?.data.text()).toBe("PNG");
  });

  it("prend le texte sinon, en UTF-8", async () => {
    const clip = await pickClip([item({ "text/plain": "Café" })]);
    expect(clip?.type).toBe("text/plain;charset=utf-8");
    expect(await clip?.data.text()).toBe("Café");
  });

  it("convertit en PNG les formats d'image que le PC ne lit pas", async () => {
    const toPng = vi.fn(async () => new Blob(["converti"], { type: "image/png" }));
    const clip = await pickClip([item({ "image/heic": "HEIC" })], toPng);
    expect(toPng).toHaveBeenCalledOnce();
    expect(clip?.type).toBe("image/png");
  });

  it("ignore un texte vide et renvoie null si rien n'est utilisable", async () => {
    expect(await pickClip([item({ "text/plain": "" })])).toBeNull();
    expect(await pickClip([item({ "application/x-custom": "?" })])).toBeNull();
    expect(await pickClip([])).toBeNull();
  });
});

describe("readPhoneClipboard", () => {
  it("lit le presse-papiers riche quand le navigateur le permet", async () => {
    const clip = await readPhoneClipboard({
      read: async () => [item({ "image/jpeg": "JPG" })] as never,
      readText: async () => "jamais",
    });
    expect(clip?.type).toBe("image/jpeg");
  });

  it("se rabat sur le texte seul quand read() n'existe pas", async () => {
    const clip = await readPhoneClipboard({ readText: async () => "bonjour" } as never);
    expect(await clip?.data.text()).toBe("bonjour");
  });

  it("transforme un refus de l'utilisateur en message clair", async () => {
    const denied = Object.assign(new Error("denied"), { name: "NotAllowedError" });
    await expect(
      readPhoneClipboard({
        read: async () => {
          throw denied;
        },
        readText: async () => "",
      }),
    ).rejects.toThrow(/Coller/);
  });
});

describe("sendClip", () => {
  const clip = { type: "image/png", data: new Blob(["PNG"]) };

  it("envoie le contenu brut avec son type et le jeton de l'appareil", async () => {
    const fetchFn = vi.fn(async () => new Response(JSON.stringify({ ok: true }), { status: 200 }));
    await sendClip(clip, "tok", fetchFn);
    const [url, init] = fetchFn.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toBe("/clipboard");
    expect(init.method).toBe("POST");
    expect(init.headers).toEqual({ "Content-Type": "image/png", Authorization: "Bearer tok" });
    expect(init.body).toBe(clip.data);
  });

  it("remonte le message d'erreur du PC", async () => {
    const fetchFn = async () =>
      new Response(JSON.stringify({ ok: false, message: "option désactivée" }), { status: 403 });
    await expect(sendClip(clip, "tok", fetchFn)).rejects.toThrow("option désactivée");
  });

  it("explique une réponse sans JSON (contenu trop gros)", async () => {
    const fetchFn = async () => new Response("Payload Too Large", { status: 413 });
    await expect(sendClip(clip, "tok", fetchFn)).rejects.toThrow(/volumineux/);
  });

  it("explique une coupure réseau", async () => {
    const fetchFn = async () => {
      throw new TypeError("Load failed");
    };
    await expect(sendClip(clip, "tok", fetchFn)).rejects.toThrow(/PC/);
  });
});

describe("clipFromDataTransfer", () => {
  it("extrait une image collée", () => {
    const file = new File(["PNG"], "image.png", { type: "image/png" });
    const clip = clipFromDataTransfer({ files: [file] as unknown as FileList, getData: () => "" });
    expect(clip?.type).toBe("image/png");
    expect(clip?.data).toBe(file);
  });

  it("ignore un collage de texte seul (le clavier le tape déjà)", () => {
    expect(clipFromDataTransfer({ files: [] as unknown as FileList, getData: () => "abc" })).toBeNull();
  });
});

describe("shouldOfferOnReturn", () => {
  it("propose l'envoi en revenant dans l'app après un passage ailleurs", () => {
    expect(shouldOfferOnReturn(true, 0, 5000)).toBe(true);
  });
  it("pas pour un simple clignement ni quand l'option est coupée", () => {
    expect(shouldOfferOnReturn(true, 0, 400)).toBe(false);
    expect(shouldOfferOnReturn(false, 0, 5000)).toBe(false);
  });
});
