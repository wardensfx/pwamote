import { describe, expect, it } from "vitest";
import {
  buttonProblems,
  keyFromEvent,
  modifiersFromEvent,
  move,
  newButton,
  newInput,
  newPage,
  occupiedExtent,
  resizePage,
  shortcutLabel,
  swapCells,
  toFieldId,
  unknownFields,
  type ActionButton,
} from "./deck";

const b = (id: string): ActionButton => ({ ...newButton(id), label: id });

describe("grille", () => {
  it("garde chaque bouton sur sa ligne et sa colonne quand on agrandit", () => {
    const p = newPage(0, "p");
    p.cells[1] = b("x"); // ligne 0, colonne 1
    p.cells[5] = b("y"); // ligne 1, colonne 2
    const big = resizePage(p, 4, 3);
    expect(big.cells.length).toBe(12);
    expect(big.cells[1]?.id).toBe("x");
    expect(big.cells[6]?.id).toBe("y");
  });

  it("ne réduit pas en dessous des boutons occupés", () => {
    const p = newPage(0, "p");
    p.cells[5] = b("y");
    expect(occupiedExtent(p)).toEqual({ columns: 3, rows: 2 });
    const small = resizePage(p, 2, 1);
    expect([small.columns, small.rows]).toEqual([3, 2]);
    expect(resizePage(p, 9, 9).columns).toBe(5);
  });

  it("échange deux cases", () => {
    const p = newPage(0, "p");
    p.cells[0] = b("a");
    const s = swapCells(p, 0, 4);
    expect(s.cells[0]).toBeNull();
    expect(s.cells[4]?.id).toBe("a");
  });

  it("déplace un élément dans une liste", () => {
    expect(move([1, 2, 3], 0, 2)).toEqual([2, 3, 1]);
    expect(move([1, 2, 3], 0, -1)).toEqual([1, 2, 3]);
  });
});

describe("capture des touches", () => {
  it("prend la lettre tapée, même sur un clavier AZERTY", () => {
    expect(keyFromEvent({ key: "a", code: "KeyQ" })).toBe("a");
    expect(keyFromEvent({ key: "S", code: "KeyS" })).toBe("s");
  });

  it("reconnaît les touches nommées, les chiffres et ignore les modificateurs seuls", () => {
    expect(keyFromEvent({ key: "ArrowLeft", code: "ArrowLeft" })).toBe("arrow_left");
    expect(keyFromEvent({ key: "F11", code: "F11" })).toBe("f11");
    expect(keyFromEvent({ key: "&", code: "Digit1" })).toBe("1");
    expect(keyFromEvent({ key: "Control", code: "ControlLeft" })).toBeNull();
    expect(keyFromEvent({ key: "MediaPlayPause", code: "MediaPlayPause" })).toBe("media_play_pause");
  });

  it("lit les modificateurs dans l'ordre", () => {
    expect(modifiersFromEvent({ ctrlKey: true, shiftKey: true, altKey: false, metaKey: false })).toEqual(["ctrl", "shift"]);
    expect(shortcutLabel("s", ["shift", "meta"])).toBe("Maj + Win + S");
    expect(shortcutLabel("page_down", [])).toBe("Page suiv.");
  });
});

describe("champs et vérifications", () => {
  it("propose un nom de champ libre", () => {
    expect(newInput(["champ1"]).id).toBe("champ2");
    expect(toFieldId("Vitesse de lecture")).toBe("vitesse_de_lecture");
    expect(toFieldId("Écran n°2")).toBe("ecran_n_2");
  });

  it("signale les étapes incomplètes et les doublons", () => {
    const x: ActionButton = { ...b("x"), steps: [{ kind: "run", program: " ", args: [], cwd: "", hidden: false }], inputs: [newInput([]), newInput([])] };
    const problems = buttonProblems(x);
    expect(problems.some((p) => p.includes("programme"))).toBe(true);
    expect(problems.some((p) => p.includes("même nom"))).toBe(true);
    expect(buttonProblems({ ...b("y"), steps: [{ kind: "open", target: "{chemin}" }] }).some((p) => p.includes("commence par un champ"))).toBe(true);
    expect(buttonProblems(b("ok"))).toEqual([]);
  });

  it("repère les champs utilisés mais non déclarés", () => {
    const x: ActionButton = {
      ...b("x"),
      inputs: [{ id: "q", label: "Q", kind: { type: "text", default: "", placeholder: "" } }],
      steps: [
        { kind: "open", target: "https://x.org/?q={q:url}&p={page}" },
        { kind: "text", text: "{{litéral}}" },
      ],
    };
    expect(unknownFields(x)).toEqual(["page"]);
  });
});
