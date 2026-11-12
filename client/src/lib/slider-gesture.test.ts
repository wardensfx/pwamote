import { describe, expect, it } from "vitest";
import { SliderGesture, valueAt } from "./slider-gesture";

describe("SliderGesture", () => {
  it("laisse défiler la page quand le doigt part à la verticale", () => {
    const g = new SliderGesture();
    g.down(100, 100);
    expect(g.move(102, 104)).toBe("pending");
    expect(g.move(103, 130)).toBe("scroll");
    // Une fois le défilement reconnu, un mouvement horizontal ne change plus rien.
    expect(g.move(200, 135)).toBe("scroll");
  });

  it("déplace le curseur quand le doigt part à l'horizontale", () => {
    const g = new SliderGesture();
    g.down(100, 100);
    expect(g.move(112, 103)).toBe("drag");
    expect(g.move(130, 160)).toBe("drag");
  });

  it("ne change pas la valeur sur un simple toucher (par exemple pour arrêter un défilement)", () => {
    const g = new SliderGesture();
    g.down(100, 100);
    g.move(102, 101);
    expect(g.up()).toBe("pending");
  });

  it("repart de zéro à chaque nouveau toucher", () => {
    const g = new SliderGesture();
    g.down(0, 0);
    g.move(0, 50);
    g.up();
    g.down(0, 0);
    expect(g.move(20, 0)).toBe("drag");
  });

  it("ignore les mouvements sans toucher en cours", () => {
    const g = new SliderGesture();
    expect(g.move(50, 0)).toBe("idle");
  });
});

describe("valueAt", () => {
  // Piste de 226 px dont 13 px de chaque côté sont occupés par la moitié du bouton.
  const track = { left: 0, width: 226, inset: 13 };

  it("convertit la position du doigt en valeur arrondie au pas", () => {
    expect(valueAt(13, track, 0, 100, 1)).toBe(0);
    expect(valueAt(213, track, 0, 100, 1)).toBe(100);
    expect(valueAt(113, track, 0, 100, 1)).toBe(50);
    expect(valueAt(113, track, 0, 3, 0.05)).toBe(1.5);
  });

  it("borne la valeur aux extrémités", () => {
    expect(valueAt(-40, track, 5, 500, 1)).toBe(5);
    expect(valueAt(400, track, 5, 500, 1)).toBe(500);
  });

  it("respecte le pas depuis le minimum sans bruit d'arrondi", () => {
    expect(valueAt(13 + 200 * 0.3, track, 1.25, 6, 0.25)).toBe(2.75);
    expect(valueAt(13 + 200 * 0.1, track, 0, 1, 0.05)).toBe(0.1);
  });
});
