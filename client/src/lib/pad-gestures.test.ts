import { describe, expect, it } from "vitest";
import type { PointerMode } from "../generated/PointerMode";
import { PadGestures } from "./pad-gestures";

function setup(mode: PointerMode) {
  const log: string[] = [];
  const g = new PadGestures(
    {
      clutch: (e) => log.push(`clutch ${e}`),
      trackpad: (dx, dy) => log.push(`trackpad ${dx} ${dy}`),
      scroll: (dx, dy) => log.push(`scroll ${dx} ${dy}`),
      click: () => log.push("click"),
    },
    () => mode,
  );
  return { g, log };
}

describe("PadGestures", () => {
  it("un doigt posé embraye le gyroscope, levé il le débraye", () => {
    const { g, log } = setup("air_hold");
    g.down(1, 10, 10, 0);
    g.move(1, 30, 12);
    g.up(1, 500);
    expect(log).toEqual(["clutch true", "clutch false"]);
  });

  it("en mode pouce, glisser verticalement défile et lâche le curseur", () => {
    const { g, log } = setup("air_hold");
    g.down(1, 100, 100, 0);
    g.move(1, 100, 120);
    g.move(1, 101, 150);
    g.move(1, 101, 170);
    g.up(1, 900);
    expect(log).toEqual(["clutch true", "clutch false", "scroll 0 30", "scroll 0 20", "clutch false"]);
  });

  it("en mode pouce, un petit tremblement ou un glissement horizontal ne défile pas", () => {
    const { g, log } = setup("air_hold");
    g.down(1, 100, 100, 0);
    g.move(1, 104, 103);
    g.move(1, 118, 128);
    g.move(1, 140, 135);
    g.up(1, 900);
    expect(log).toEqual(["clutch true", "clutch false"]);
  });

  it("en mode gyro continu, le doigt ne défile pas", () => {
    const { g, log } = setup("air_always");
    g.down(1, 0, 0, 0);
    g.move(1, 0, 80);
    g.up(1, 900);
    expect(log).toEqual(["clutch true", "clutch false"]);
  });

  it("en mode trackpad, le doigt déplace le curseur", () => {
    const { g, log } = setup("trackpad");
    g.down(1, 0, 0, 0);
    g.move(1, 5, -3);
    g.up(1, 900);
    expect(log).toEqual(["clutch true", "trackpad 5 -3", "clutch false"]);
  });

  it("un tap bref en mode trackpad fait un clic", () => {
    const { g, log } = setup("trackpad");
    g.down(1, 0, 0, 0);
    g.up(1, 120);
    expect(log).toEqual(["clutch true", "click", "clutch false"]);
  });

  it("un tap trop long ou avec trop de mouvement n'est pas un clic", () => {
    const slow = setup("trackpad");
    slow.g.down(1, 0, 0, 0);
    slow.g.up(1, 400);
    expect(slow.log).not.toContain("click");
    const moved = setup("trackpad");
    moved.g.down(1, 0, 0, 0);
    moved.g.move(1, 20, 0);
    moved.g.up(1, 100);
    expect(moved.log).not.toContain("click");
  });

  it("pas de clic par tap en mode gyro continu", () => {
    const { g, log } = setup("air_always");
    g.down(1, 0, 0, 0);
    g.up(1, 50);
    expect(log).not.toContain("click");
  });

  it("deux doigts font défiler, dans tous les modes", () => {
    const { g, log } = setup("air_hold");
    g.down(1, 0, 0, 0);
    g.down(2, 50, 0, 10);
    g.move(1, 0, 20);
    expect(log).toContain("scroll 0 10");
    g.up(2, 100);
    g.up(1, 120);
    expect(log.at(-1)).toBe("clutch false");
  });

  it("les deux doigts levés après un défilement ne cliquent pas", () => {
    const { g, log } = setup("trackpad");
    g.down(1, 0, 0, 0);
    g.down(2, 50, 0, 5);
    g.up(2, 10);
    g.up(1, 20);
    expect(log).not.toContain("click");
  });

  it("cancel débraye proprement", () => {
    const { g, log } = setup("air_hold");
    g.down(1, 0, 0, 0);
    g.cancel();
    expect(log).toEqual(["clutch true", "clutch false"]);
  });

  it("en mode pouce, un petit mouvement du pouce pendant qu'on vise ne coupe pas le curseur", () => {
    const { g, log } = setup("air_hold");
    g.down(1, 100, 100, 0);
    g.move(1, 100, 118);
    g.move(1, 102, 128);
    g.up(1, 900);
    expect(log).toEqual(["clutch true", "clutch false"]);
  });

  it("en mode pouce, un tap clique et deux taps cliquent deux fois", () => {
    const { g, log } = setup("air_hold");
    g.down(1, 50, 50, 0);
    g.up(1, 90);
    g.down(1, 50, 50, 200);
    g.up(1, 250);
    expect(log).toEqual(["clutch true", "click", "clutch false", "clutch true", "click", "clutch false"]);
  });

  it("en mode pouce, un appui long pour viser ne clique pas", () => {
    const { g, log } = setup("air_hold");
    g.down(1, 50, 50, 0);
    g.up(1, 400);
    expect(log).toEqual(["clutch true", "clutch false"]);
  });
});
