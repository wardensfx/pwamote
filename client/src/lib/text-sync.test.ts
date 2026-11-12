import { describe, expect, it } from "vitest";
import { TextSync, isAndroid } from "./text-sync";

function setup(waitForComposition: boolean) {
  const out: string[] = [];
  const sync = new TextSync({ backspace: (n) => out.push(`⌫${n}`), text: (t) => out.push(t) }, { waitForComposition });
  return { out, sync };
}

describe("TextSync sur Android (Gboard compose chaque mot)", () => {
  it("envoie les lettres au fil de la frappe, même pendant la composition", () => {
    const { out, sync } = setup(false);
    sync.compositionStart();
    sync.input("B");
    sync.input("Bo");
    sync.input("Bon");
    expect(out).toEqual(["B", "o", "n"]);
  });

  it("suit l'autocorrection du mot en cours", () => {
    const { out, sync } = setup(false);
    sync.compositionStart();
    sync.input("Bonjuor");
    sync.input("Bonjour ");
    sync.compositionEnd("Bonjour ");
    expect(out).toEqual(["Bonjuor", "⌫3", "our "]);
  });

  it("Entrée pendant la composition envoie d'abord le mot en cours", () => {
    const { out, sync } = setup(true);
    sync.compositionStart();
    sync.input("salut");
    sync.flush("salut");
    expect(out).toEqual(["salut"]);
    sync.reset();
    // La fin de composition qui suit l'effacement du champ n'envoie rien de plus.
    sync.compositionEnd("");
    expect(out).toEqual(["salut"]);
  });
});

describe("TextSync sur iPhone et ordinateur", () => {
  it("attend la fin de la composition (accents à touche morte, japonais…)", () => {
    const { out, sync } = setup(true);
    sync.compositionStart();
    sync.input("^");
    sync.input("ê");
    expect(out).toEqual([]);
    sync.compositionEnd("ê");
    expect(out).toEqual(["ê"]);
  });

  it("envoie directement hors composition", () => {
    const { out, sync } = setup(true);
    sync.input("a");
    sync.input("ab");
    sync.input("a");
    expect(out).toEqual(["a", "b", "⌫1"]);
  });
});

describe("TextSync", () => {
  it("signale quand le champ est assez long pour repartir de zéro", () => {
    const { sync } = setup(false);
    expect(sync.input("x".repeat(300))).toBe(false);
    expect(sync.input("x".repeat(301))).toBe(true);
  });
});

describe("isAndroid", () => {
  it("reconnaît Chrome sur Android", () => {
    expect(isAndroid("Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 Chrome/131 Mobile Safari/537.36")).toBe(true);
    expect(isAndroid("Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15")).toBe(false);
  });
});
