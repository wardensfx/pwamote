import { describe, expect, it } from "vitest";
import { MotionSource, normalizeSample, rotateForOrientation } from "./motion";

describe("normalizeSample", () => {
  it("convertit le repère d'iOS vers le repère W3C attendu par le serveur", () => {
    // iOS : rotationRate {alpha: z, beta: x, gamma: y}, gravité de signe inverse.
    const s = normalizeSample(
      { rotationRate: { alpha: 30, beta: 10, gamma: 20 }, accelerationIncludingGravity: { x: 0, y: 0, z: -9.81 } },
      0.016,
      { invertAccel: true },
    );
    expect(s.gyro).toEqual([10, 20, 30]);
    expect(s.accel).toEqual([-0, -0, 9.81]);
    expect(s.dt).toBe(0.016);
  });

  it("garde le signe de la gravité hors iOS", () => {
    const s = normalizeSample(
      { rotationRate: { alpha: 0, beta: 0, gamma: 0 }, accelerationIncludingGravity: { x: 1, y: 2, z: 9.81 } },
      0.01,
      { invertAccel: false },
    );
    expect(s.accel).toEqual([1, 2, 9.81]);
  });

  it("tolère les valeurs manquantes", () => {
    const s = normalizeSample({ rotationRate: null, accelerationIncludingGravity: null }, 0.01, { invertAccel: false });
    expect(s.gyro).toEqual([0, 0, 0]);
    expect(s.accel).toEqual([0, 0, 0]);
  });
});

describe("rotateForOrientation", () => {
  const v: [number, number, number] = [1, 2, 3];
  it("ne change rien en portrait", () => expect(rotateForOrientation(v, 0)).toEqual([1, 2, 3]));
  it("tourne x/y en paysage", () => {
    expect(rotateForOrientation(v, 90)).toEqual([-2, 1, 3]);
    expect(rotateForOrientation(v, 270)).toEqual([2, -1, 3]);
    expect(rotateForOrientation(v, 180)).toEqual([-1, -2, 3]);
  });
});

describe("MotionSource", () => {
  it("calcule dt à partir de l'horloge et ignore le premier événement", () => {
    const got: number[] = [];
    let now = 1000;
    const src = new MotionSource((s) => got.push(s.dt), { now: () => now, invertAccel: false, angle: () => 0 });
    const ev = { rotationRate: { alpha: 0, beta: 0, gamma: 0 }, accelerationIncludingGravity: { x: 0, y: 0, z: 9.81 } };
    src.handle(ev);
    now += 16;
    src.handle(ev);
    now += 17;
    src.handle(ev);
    expect(got.map((d) => Math.round(d * 1000))).toEqual([16, 17]);
  });
});
