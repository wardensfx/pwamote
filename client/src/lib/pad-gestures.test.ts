import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
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
      press: (down) => log.push(`press ${down}`),
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

  it("en mode pouce, une dérive lente du pouce, même longue, ne défile jamais", () => {
    const { g, log } = setup("air_hold");
    g.down(1, 100, 100, 0);
    // 4 px toutes les 100 ms : 120 px au bout de 3 s, mais jamais 40 px en 250 ms.
    for (let i = 1; i <= 30; i++) g.move(1, 100, 100 + 4 * i, i * 100);
    g.up(1, 3100);
    expect(log).toEqual(["clutch true", "clutch false"]);
  });

  it("en mode pouce, un balayage vertical rapide défile même après une longue visée", () => {
    const { g, log } = setup("air_hold");
    g.down(1, 100, 100, 0);
    g.move(1, 102, 104, 1000);
    g.move(1, 102, 130, 1050);
    g.move(1, 102, 160, 1100);
    g.up(1, 1500);
    expect(log).toContain("scroll 0 30");
  });
});

describe("PadGestures en gyro continu : la surface est le bouton gauche", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("un tap fait un clic gauche", () => {
    const { g, log } = setup("air_always");
    g.down(1, 50, 50, 0);
    g.up(1, 90);
    vi.runAllTimers();
    expect(log.filter((l) => !l.startsWith("clutch"))).toEqual(["click"]);
  });

  it("deux taps rapprochés cliquent deux fois (double clic)", () => {
    const { g, log } = setup("air_always");
    g.down(1, 50, 50, 0);
    g.up(1, 80);
    g.down(1, 50, 50, 150);
    g.up(1, 230);
    expect(log.filter((l) => l === "click")).toHaveLength(2);
  });

  it("un appui maintenu enfonce le bouton jusqu'au lever du doigt (glisser-déposer)", () => {
    const { g, log } = setup("air_always");
    g.down(1, 50, 50, 0);
    vi.advanceTimersByTime(300);
    expect(log).toContain("press true");
    // Le pouce bouge un peu pendant qu'on vise : le bouton reste enfoncé, rien ne défile.
    g.move(1, 52, 70, 400);
    g.move(1, 52, 110, 450);
    g.up(1, 1200);
    expect(log.filter((l) => !l.startsWith("clutch"))).toEqual(["press true", "press false"]);
  });

  it("glisser vite à un doigt verticalement défile sans cliquer", () => {
    const { g, log } = setup("air_always");
    g.down(1, 100, 100, 0);
    g.move(1, 100, 120, 40);
    g.move(1, 101, 150, 80);
    g.move(1, 101, 170, 120);
    vi.advanceTimersByTime(500);
    g.up(1, 900);
    const out = log.filter((l) => !l.startsWith("clutch"));
    expect(out).toEqual(["scroll 0 30", "scroll 0 20"]);
  });

  it("un glissement horizontal bref reste un clic", () => {
    const { g, log } = setup("air_always");
    g.down(1, 100, 100, 0);
    g.move(1, 130, 104, 60);
    g.up(1, 120);
    expect(log).toContain("click");
    expect(log.some((l) => l.startsWith("scroll"))).toBe(false);
  });

  it("deux doigts défilent sans cliquer ni enfoncer le bouton", () => {
    const { g, log } = setup("air_always");
    g.down(1, 0, 0, 0);
    g.down(2, 50, 0, 10);
    g.move(1, 0, 20, 30);
    vi.advanceTimersByTime(500);
    g.up(2, 600);
    g.up(1, 620);
    expect(log).toContain("scroll 0 10");
    expect(log).not.toContain("click");
    expect(log).not.toContain("press true");
  });

  it("cancel relâche un bouton enfoncé", () => {
    const { g, log } = setup("air_always");
    g.down(1, 0, 0, 0);
    vi.advanceTimersByTime(300);
    g.cancel();
    expect(log).toContain("press false");
    vi.runAllTimers();
    expect(log.filter((l) => l === "press true")).toHaveLength(1);
  });

  it("le doigt ne déplace jamais le curseur", () => {
    const { g, log } = setup("air_always");
    g.down(1, 0, 0, 0);
    g.move(1, 40, 3, 50);
    g.up(1, 900);
    expect(log.some((l) => l.startsWith("trackpad"))).toBe(false);
  });
});
