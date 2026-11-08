import { describe, expect, it } from "vitest";
import { AxisCalibrator, CANDIDATES, W3C_MAP, applyMap, type AxisMap } from "./axis-calibration";
import type { Vec3 } from "./frames";

const cross = (a: Vec3, b: Vec3): Vec3 => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const norm = (a: Vec3): Vec3 => {
  const n = Math.hypot(...a);
  return [a[0] / n, a[1] / n, a[2] / n];
};

/** Simule un téléphone qui tourne : le capteur annonce `raw` selon `map`, la gravité suit la vraie rotation. */
function simulate(map: AxisMap, seconds = 4): { raw: Vec3; accel: Vec3; dt: number }[] {
  const dt = 1 / 60;
  let up: Vec3 = norm([0.2, 0.3, 1]);
  const out = [];
  for (let i = 0; i < seconds * 60; i++) {
    const t = i * dt;
    // Gestes variés : tangage, roulis et lacet à tour de rôle, avec des fréquences différentes.
    const omega: Vec3 = [70 * Math.sin(2.1 * t), 55 * Math.sin(1.3 * t + 1), 40 * Math.sin(0.9 * t + 2)];
    const rad: Vec3 = [(omega[0] * Math.PI) / 180, (omega[1] * Math.PI) / 180, (omega[2] * Math.PI) / 180];
    const d = cross(rad, up);
    up = norm([up[0] - d[0] * dt, up[1] - d[1] * dt, up[2] - d[2] * dt]);
    // raw[map.perm[i]] * sign = omega[i]  ⇒  raw[map.perm[i]] = sign * omega[i]
    const raw: Vec3 = [0, 0, 0];
    map.perm.forEach((p, i) => (raw[p] = map.sign * omega[i]!));
    out.push({ raw, accel: [up[0] * 9.81, up[1] * 9.81, up[2] * 9.81] as Vec3, dt });
  }
  return out;
}

describe("AxisCalibrator", () => {
  it("retrouve n'importe quelle affectation d'axes à partir des seuls capteurs", () => {
    for (const truth of CANDIDATES) {
      const cal = new AxisCalibrator();
      for (const s of simulate(truth)) cal.push(s.raw, s.accel, s.dt);
      expect(cal.best(), JSON.stringify(truth)).toEqual(truth);
    }
  });

  it("ne tranche pas sans mouvement", () => {
    const cal = new AxisCalibrator();
    for (let i = 0; i < 300; i++) cal.push([0.1, 0, 0], [0, 0, 9.81], 1 / 60);
    expect(cal.best()).toBeNull();
  });

  it("applique l'affectation : x = beta, y = gamma, z = alpha en W3C", () => {
    expect(applyMap([30, 10, 20], W3C_MAP)).toEqual([10, 20, 30]);
  });

  it("ignore une accélération nulle sans planter", () => {
    const cal = new AxisCalibrator();
    for (let i = 0; i < 10; i++) cal.push([90, 0, 0], [0, 0, 0], 1 / 60);
    expect(cal.best()).toBeNull();
  });
});
