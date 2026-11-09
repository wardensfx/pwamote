import { describe, expect, it, vi } from "vitest";
import { clipFromDataTransfer, sendClip, toPcFormat } from "./clipboard";

describe("toPcFormat", () => {
  it("laisse passer les formats que le PC décode", async () => {
    const clip = { type: "image/jpeg", data: new Blob(["JPG"]) };
    expect(await toPcFormat(clip, async () => new Blob())).toBe(clip);
  });

  it("convertit en PNG les autres images (HEIC…)", async () => {
    const toPng = vi.fn(async () => new Blob(["converti"], { type: "image/png" }));
    const clip = await toPcFormat({ type: "image/heic", data: new Blob(["HEIC"]) }, toPng);
    expect(toPng).toHaveBeenCalledOnce();
    expect(clip.type).toBe("image/png");
  });
});

describe("sendClip", () => {
  const clip = { type: "image/png", data: new Blob(["PNG"]) };

  it("envoie le contenu brut avec son type et le jeton de l'appareil", async () => {
    const fetchFn = vi.fn(async () => new Response(JSON.stringify({ ok: true }), { status: 200 }));
    await sendClip(clip, "tok", fetchFn);
    const [url, init] = fetchFn.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toBe("/paste");
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
