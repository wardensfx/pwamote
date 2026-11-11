import { describe, expect, it } from "vitest";
import { MAX_MESSAGE, canSend, emailOk, fetchReportInfo, phoneOs, reportLang, sendReport, withPhone } from "./report";

describe("signaler un problème", () => {
  it("décrit le système du téléphone sans rien d'identifiant", () => {
    expect(phoneOs("Mozilla/5.0 (iPhone; CPU iPhone OS 18_1_1 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.1 Mobile/15E148 Safari/604.1")).toBe("iOS 18.1.1");
    expect(phoneOs("Mozilla/5.0 (iPad; CPU OS 17_4 like Mac OS X) AppleWebKit/605.1.15")).toBe("iPadOS 17.4");
    expect(phoneOs("Mozilla/5.0 (Linux; Android 15; Pixel 8) AppleWebKit/537.36 Chrome/131.0 Mobile Safari/537.36")).toBe("Android 15");
    // iPad récent : se présente comme un Mac, avec l'écran tactile.
    expect(phoneOs("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 Version/18.0 Safari/605.1.15", 5)).toBe("iPadOS");
    expect(phoneOs("Mozilla/5.0 (X11; Linux x86_64)")).toBe("Linux");
    expect(phoneOs("bizarre")).toBe("");
  });
  it("contrôle le formulaire", () => {
    expect(emailOk("")).toBe(true);
    expect(emailOk(" jean@exemple.fr ")).toBe(true);
    expect(emailOk("jean@exemple")).toBe(false);
    expect(emailOk("pas une@adresse.fr")).toBe(false);
    expect(canSend("Le curseur saute", "")).toBe(true);
    expect(canSend("   ", "")).toBe(false);
    expect(canSend("ok", "faux")).toBe(false);
    expect(canSend("x".repeat(MAX_MESSAGE + 1), "")).toBe(false);
  });
  it("ajoute le système du téléphone à l'issue préremplie", () => {
    const base = "https://github.com/wardensfx/pwamote/issues/new?template=bug.yml&version=0.4.2";
    expect(withPhone(base, "iOS 18.1")).toBe(`${base}&phone=iOS+18.1`);
    expect(withPhone(base, "")).toBe(base);
  });
  it("langue de la réponse d'après le navigateur", () => {
    expect(reportLang("en-GB")).toBe("en");
    expect(reportLang("fr-FR")).toBe("fr");
    expect(reportLang("de")).toBe("en");
    expect(reportLang(undefined)).toBe("fr");
  });
});

describe("échanges avec le PC", () => {
  const reply = (status: number, body: unknown) => async () => new Response(JSON.stringify(body), { status });
  it("lit le canal du PC avec le jeton du téléphone", async () => {
    let auth = "";
    const info = await fetchReportInfo("tok", async (_u, init) => {
      auth = new Headers(init?.headers).get("authorization") ?? "";
      return new Response(JSON.stringify({ channel: "cloud", issue_url: "https://github.com/x/y/issues/new", version: "0.4.2", os: "Windows 11" }));
    });
    expect(auth).toBe("Bearer tok");
    expect(info.channel).toBe("cloud");
  });
  it("renvoie la référence du ticket ou une erreur lisible", async () => {
    expect(await sendReport("tok", { message: "x", include_logs: false }, reply(200, { ok: true, ticket: "K7Q2MX" }))).toBe("K7Q2MX");
    await expect(sendReport("tok", { message: "x", include_logs: false }, reply(429, { ok: false, message: "trop de signalements, réessaie plus tard" }))).rejects.toThrow("trop de signalements");
    await expect(sendReport("tok", { message: "x", include_logs: false }, async () => { throw new Error("réseau"); })).rejects.toThrow("Le PC ne répond pas.");
    await expect(fetchReportInfo("tok", reply(404, {}))).rejects.toThrow(/PC/);
  });
});
