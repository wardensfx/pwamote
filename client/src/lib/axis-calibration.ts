import type { Vec3 } from "./frames";

/**
 * Affectation des trois vitesses de rotation brutes (alpha, beta, gamma) aux axes
 * du téléphone (x, y, z) : `omega[i] = sign * raw[perm[i]]`.
 *
 * La spécification W3C dit alpha = z, beta = x, gamma = y. Selon les versions du
 * navigateur, ce n'est pas ce que le capteur fournit (Safari a longtemps annoncé
 * les axes dans l'ordre de Core Motion). Plutôt que de deviner, on le mesure.
 */
export interface AxisMap {
  perm: [number, number, number];
  sign: 1 | -1;
}

export const W3C_MAP: AxisMap = { perm: [1, 2, 0], sign: 1 };
export const IDENTITY_MAP: AxisMap = { perm: [0, 1, 2], sign: 1 };

const PERMS: [number, number, number][] = [
  [0, 1, 2],
  [0, 2, 1],
  [1, 0, 2],
  [1, 2, 0],
  [2, 0, 1],
  [2, 1, 0],
];
export const CANDIDATES: AxisMap[] = PERMS.flatMap((perm) => [
  { perm, sign: 1 as const },
  { perm, sign: -1 as const },
]);

export function applyMap(raw: Vec3, map: AxisMap): Vec3 {
  return [map.sign * raw[map.perm[0]]!, map.sign * raw[map.perm[1]]!, map.sign * raw[map.perm[2]]!];
}

const cross = (a: Vec3, b: Vec3): Vec3 => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];

/** Vitesse minimale (°/s) pour qu'un échantillon apporte de l'information. */
const MIN_RATE = 25;
/** Variation cumulée de la gravité (unité : vecteur unitaire) avant de se prononcer. */
const MIN_EVIDENCE = 0.6;
/** Le meilleur candidat doit avoir une erreur au plus égale à cette part du deuxième. */
const MARGIN = 0.6;

/**
 * Compare, pour chaque affectation possible, la rotation annoncée par le
 * gyroscope à la rotation réellement subie par la gravité mesurée :
 * `d(g)/dt = -ω × g`. La bonne affectation est celle qui s'accorde le mieux.
 * Le signe de l'accéléromètre n'intervient pas : la relation est linéaire.
 */
export class AxisCalibrator {
  private prev: Vec3 | null = null;
  private errors = new Array<number>(CANDIDATES.length).fill(0);
  private evidence = 0;

  push(raw: Vec3, accel: Vec3, dt: number): void {
    const n = Math.hypot(...accel);
    if (!(n > 1) || !(dt > 0)) {
      this.prev = null;
      return;
    }
    const g: Vec3 = [accel[0] / n, accel[1] / n, accel[2] / n];
    const prev = this.prev;
    this.prev = g;
    if (!prev || Math.hypot(...raw) < MIN_RATE) return;

    const observed: Vec3 = [g[0] - prev[0], g[1] - prev[1], g[2] - prev[2]];
    this.evidence += Math.hypot(...observed);
    CANDIDATES.forEach((c, i) => {
      const w = applyMap(raw, c).map((v) => (v * Math.PI) / 180) as Vec3;
      const x = cross(w, prev);
      const rx = observed[0] + x[0] * dt;
      const ry = observed[1] + x[1] * dt;
      const rz = observed[2] + x[2] * dt;
      this.errors[i]! += rx * rx + ry * ry + rz * rz;
    });
  }

  /** L'affectation la plus probable, ou `null` tant que les preuves manquent. */
  best(): AxisMap | null {
    if (this.evidence < MIN_EVIDENCE) return null;
    const ranked = this.errors.map((e, i) => [e, i] as const).sort((a, b) => a[0] - b[0]);
    const [best, second] = [ranked[0]!, ranked[1]!];
    if (best[0] > MARGIN * second[0]) return null;
    return CANDIDATES[best[1]]!;
  }

  reset(): void {
    this.prev = null;
    this.errors.fill(0);
    this.evidence = 0;
  }
}
