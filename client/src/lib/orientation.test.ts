import { describe, expect, it } from "vitest";
import { OrientationSource, angularVelocity, quatFromEuler, quatMul, upInBody, type Quat } from "./orientation";
import type { MotionSample } from "./frames";

const RAD = Math.PI / 180;

/** Rotation de `deg` degrés autour d'un axe du repère de l'appareil. */
const rot = (axis: 0 | 1 | 2, deg: number): Quat => {
  const q: Quat = [Math.cos((deg * RAD) / 2), 0, 0, 0];
  q[axis + 1] = Math.sin((deg * RAD) / 2);
  return q;
};

/** Convertit une orientation en angles W3C (ZXY), hors zone de blocage. */
function toEuler(q: Quat) {
  const [w, x, y, z] = q;
  const r01 = 2 * (x * y - w * z);
  const r11 = 1 - 2 * (x * x + z * z);
  const r20 = 2 * (x * z - w * y);
  const r21 = 2 * (y * z + w * x);
  const r22 = 1 - 2 * (x * x + y * y);
  return {
    alpha: ((Math.atan2(-r01, r11) / RAD) + 360) % 360,
    beta: Math.asin(r21) / RAD,
    gamma: Math.atan2(-r20, r22) / RAD,
  };
}

function run(start: Quat, omega: [number, number, number], seconds = 0.5, hz = 60) {
  const samples: MotionSample[] = [];
  let t = 0;
  const src = new OrientationSource((s) => samples.push(s), { now: () => t, angle: () => 0 });
  let q = start;
  const dt = 1 / hz;
  const step = quatMul(quatMul(rot(0, omega[0] * dt), rot(1, omega[1] * dt)), rot(2, omega[2] * dt));
  for (let i = 0; i <= seconds * hz; i++) {
    const e = toEuler(q);
    src.handle(e);
    q = quatMul(q, step);
    t += dt * 1000;
  }
  return samples;
}

describe("orientation", () => {
  it("téléphone à plat : le haut est l'axe z", () => {
    const u = upInBody(0, 0);
    expect(u[0]).toBeCloseTo(0);
    expect(u[1]).toBeCloseTo(0);
    expect(u[2]).toBeCloseTo(1);
  });

  it("téléphone debout (beta 90) : le haut est l'axe y", () => {
    const u = upInBody(90, 0);
    expect(u[1]).toBeCloseTo(1);
    expect(u[2]).toBeCloseTo(0, 5);
  });

  it("couché sur le côté droit (gamma 90) : le haut est -x", () => {
    expect(upInBody(0, 90)[0]).toBeCloseTo(-1);
  });

  it("la rotation relative donne la vitesse angulaire autour de chaque axe de l'appareil", () => {
    const start = quatFromEuler(30, 20, -10);
    for (const [axis, omega] of [
      [0, [40, 0, 0]],
      [1, [0, -30, 0]],
      [2, [0, 0, 55]],
    ] as const) {
      const s = run(start, [omega[0], omega[1], omega[2]]);
      const mid = s[s.length >> 1]!.gyro;
      for (let i = 0; i < 3; i++) expect(mid[i]).toBeCloseTo(omega[i]!, 0);
      expect(axis).toBeGreaterThanOrEqual(0);
    }
  });

  it("à plat, basculer le téléphone vers l'arrière (tangage) tourne autour de x", () => {
    const s = run(quatFromEuler(0, 0, 0), [30, 0, 0]);
    expect(s[10]!.gyro[0]).toBeCloseTo(30, 0);
    expect(Math.abs(s[10]!.gyro[1])).toBeLessThan(0.5);
    expect(Math.abs(s[10]!.gyro[2])).toBeLessThan(0.5);
  });

  it("à plat, tourner sur soi (lacet) tourne autour de z, quel que soit le cap", () => {
    const s = run(quatFromEuler(200, 0, 0), [0, 0, 45]);
    expect(s[10]!.gyro[2]).toBeCloseTo(45, 0);
  });

  it("la gravité reste cohérente avec la vitesse angulaire : du/dt = -ω × u", () => {
    const s = run(quatFromEuler(10, 35, 15), [20, -25, 15]);
    for (let i = 2; i < s.length - 2; i++) {
      const [a, b, c] = [s[i - 1]!, s[i]!, s[i + 1]!];
      const dt = b.dt * 2;
      const u = b.accel;
      const w = b.gyro.map((v) => v * RAD) as [number, number, number];
      const cross = [w[1] * u[2] - w[2] * u[1], w[2] * u[0] - w[0] * u[2], w[0] * u[1] - w[1] * u[0]];
      for (let k = 0; k < 3; k++) expect((c.accel[k]! - a.accel[k]!) / dt).toBeCloseTo(-cross[k]!, 0);
    }
  });

  it("franchit le passage de 359° à 0° sans saut", () => {
    const samples: MotionSample[] = [];
    let t = 0;
    const src = new OrientationSource((s) => samples.push(s), { now: () => t, angle: () => 0 });
    for (const alpha of [358, 359, 0, 1, 2]) {
      src.handle({ alpha, beta: 0, gamma: 0 });
      t += 16.67;
    }
    for (const s of samples) expect(s.gyro[2]).toBeCloseTo(60, -1);
  });

  it("ignore les événements sans valeur", () => {
    const samples: MotionSample[] = [];
    const src = new OrientationSource((s) => samples.push(s), { now: () => 0 });
    src.handle({ alpha: null, beta: null, gamma: null });
    expect(src.received).toBe(0);
    expect(samples).toHaveLength(0);
  });

  it("angularVelocity est nulle sans rotation", () => {
    const q = quatFromEuler(5, 6, 7);
    expect(angularVelocity(q, q, 0.01)).toEqual([0, 0, 0]);
  });
});
