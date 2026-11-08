import { describe, expect, it } from "vitest";
import { diffText } from "./text-diff";

describe("diffText", () => {
  it("ajout simple", () => expect(diffText("Bon", "Bonj")).toEqual({ backspaces: 0, insert: "j" }));
  it("effacement", () => expect(diffText("Bonjour", "Bonjou")).toEqual({ backspaces: 1, insert: "" }));
  it("autocorrection : remplace la fin du mot", () =>
    expect(diffText("Bonjuor", "Bonjour")).toEqual({ backspaces: 3, insert: "our" }));
  it("rien à faire", () => expect(diffText("abc", "abc")).toEqual({ backspaces: 0, insert: "" }));
  it("champ vidé d'un coup", () => expect(diffText("abc", "")).toEqual({ backspaces: 3, insert: "" }));
  it("compte les caractères, pas les unités UTF-16 (emoji, accents)", () => {
    expect(diffText("Ça 👌", "Ça ")).toEqual({ backspaces: 1, insert: "" });
    expect(diffText("e", "é")).toEqual({ backspaces: 1, insert: "é" });
  });
  it("collage de texte", () => expect(diffText("", "Salut à tous")).toEqual({ backspaces: 0, insert: "Salut à tous" }));
});
