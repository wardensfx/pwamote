import { describe, expect, it } from "vitest";
import { IDENTITY_MAP, W3C_MAP } from "./axis-calibration";
import { MotionSource, defaultAxisMap, normalizeSample, preferredSource, rotateForOrientation, sampleInterval } from "./motion";

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
    const src = new MotionSource((s) => got.push(s.dt), {
      now: () => now,
      invertAccel: false,
      angle: () => 0,
      initialMap: IDENTITY_MAP,
    });
    const ev = { rotationRate: { alpha: 0, beta: 0, gamma: 0 }, accelerationIncludingGravity: { x: 0, y: 0, z: 9.81 } };
    src.handle(ev);
    now += 16;
    src.handle(ev);
    now += 17;
    src.handle(ev);
    expect(got.map((d) => Math.round(d * 1000))).toEqual([16, 17]);
  });
});

describe("affectation des axes", () => {
  it("part de l'ordre Core Motion sur iOS et de l'ordre W3C ailleurs", () => {
    expect(defaultAxisMap(true).perm).toEqual([0, 1, 2]);
    expect(defaultAxisMap(false).perm).toEqual([1, 2, 0]);
  });

  it("applique l'affectation choisie aux vitesses de rotation", () => {
    const got: number[][] = [];
    let now = 0;
    const src = new MotionSource((s) => got.push(s.gyro), {
      now: () => now,
      invertAccel: false,
      angle: () => 0,
      initialMap: IDENTITY_MAP,
    });
    const ev = { rotationRate: { alpha: 1, beta: 2, gamma: 3 }, accelerationIncludingGravity: { x: 0, y: 0, z: 9.81 } };
    src.handle(ev);
    now += 16;
    src.handle(ev);
    expect(got[0]).toEqual([1, 2, 3]);
  });
});

describe("sampleInterval", () => {
  it("lit l'intervalle du capteur en millisecondes (Chrome) ou en secondes (ancien Safari)", () => {
    expect(sampleInterval(16)).toBeCloseTo(0.016);
    expect(sampleInterval(0.016)).toBeCloseTo(0.016);
    expect(sampleInterval(5)).toBeCloseTo(0.005);
  });

  it("ignore un intervalle absent ou aberrant", () => {
    expect(sampleInterval(undefined)).toBeNull();
    expect(sampleInterval(null)).toBeNull();
    expect(sampleInterval(0)).toBeNull();
    expect(sampleInterval(500)).toBeNull();
    expect(sampleInterval(Number.NaN)).toBeNull();
  });
});

describe("MotionSource sur Android", () => {
  const rate = (alpha: number) => ({
    rotationRate: { alpha, beta: 0, gamma: 0 },
    accelerationIncludingGravity: { x: 0, y: 0, z: 9.81 },
    interval: 16,
  });

  it("utilise la cadence du capteur et non l'heure d'arrivée des événements groupés", () => {
    const got: number[] = [];
    let now = 1000;
    const src = new MotionSource((s) => got.push(s.dt), { now: () => now, invertAccel: false, angle: () => 0, initialMap: IDENTITY_MAP });
    // Chrome livre parfois les mesures par paquets : 1 ms puis 31 ms d'écart.
    src.handle(rate(10));
    now += 1;
    src.handle(rate(10));
    now += 31;
    src.handle(rate(10));
    expect(got.map((d) => Math.round(d * 1000))).toEqual([16, 16, 16]);
  });

  it("garde l'ordre d'axes du W3C sans le remettre en cause quand on le lui demande", () => {
    const calibrated: unknown[] = [];
    let now = 0;
    const src = new MotionSource(() => {}, {
      now: () => now,
      invertAccel: false,
      angle: () => 0,
      initialMap: W3C_MAP,
      trustMap: true,
      onCalibrated: (m) => calibrated.push(m),
    });
    // Rotation franche autour de z annoncée dans alpha : la gravité tourne, rien ne doit changer.
    for (let i = 0; i < 300; i++) {
      now += 16;
      const a = (i * 2 * Math.PI) / 100;
      src.handle({
        rotationRate: { alpha: 0, beta: 0, gamma: 200 },
        accelerationIncludingGravity: { x: 9.81 * Math.sin(a), y: 0, z: 9.81 * Math.cos(a) },
        interval: 16,
      });
    }
    expect(calibrated).toEqual([]);
    expect(src.axisMap).toEqual(W3C_MAP);
  });

  it("compte seulement les événements qui portent une vitesse de rotation", () => {
    const src = new MotionSource(() => {}, { invertAccel: false, angle: () => 0, initialMap: W3C_MAP, trustMap: true });
    src.handle({ rotationRate: null, accelerationIncludingGravity: { x: 0, y: 0, z: 9.81 } });
    src.handle({ rotationRate: { alpha: null, beta: null, gamma: null }, accelerationIncludingGravity: null });
    expect(src.received).toBe(0);
    src.handle(rate(1));
    expect(src.received).toBe(1);
  });

  it("transmet l'accélération sans gravité quand l'option déplacements est active", () => {
    const lin: [number, number, number][] = [];
    let now = 0;
    const src = new MotionSource(() => {}, {
      now: () => now,
      invertAccel: false,
      angle: () => 0,
      initialMap: W3C_MAP,
      trustMap: true,
      onLinear: (a) => lin.push(a),
    });
    const ev = { ...rate(0), acceleration: { x: 1, y: -2, z: 0.5 } };
    src.handle(ev);
    expect(lin).toEqual([]);
    src.setLinear(true);
    now += 16;
    src.handle(ev);
    expect(lin).toEqual([[1, -2, 0.5]]);
  });
});

describe("preferredSource", () => {
  it("garde les angles d'orientation sur iPhone et le gyroscope brut ailleurs", () => {
    expect(preferredSource(true)).toBe("orientation");
    expect(preferredSource(false)).toBe("motion");
  });
});
