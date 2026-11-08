import type { MotionSample, Vec3 } from "./frames";
import { rotateForOrientation } from "./motion";

/** Quaternion [w, x, y, z]. */
export type Quat = [number, number, number, number];

const RAD = Math.PI / 180;
const G = 9.81;

export function quatMul(a: Quat, b: Quat): Quat {
  return [
    a[0] * b[0] - a[1] * b[1] - a[2] * b[2] - a[3] * b[3],
    a[0] * b[1] + a[1] * b[0] + a[2] * b[3] - a[3] * b[2],
    a[0] * b[2] - a[1] * b[3] + a[2] * b[0] + a[3] * b[1],
    a[0] * b[3] + a[1] * b[2] - a[2] * b[1] + a[3] * b[0],
  ];
}

export const quatConj = (q: Quat): Quat => [q[0], -q[1], -q[2], -q[3]];

const axisQuat = (axis: 0 | 1 | 2, deg: number): Quat => {
  const h = (deg * RAD) / 2;
  const q: Quat = [Math.cos(h), 0, 0, 0];
  q[axis + 1] = Math.sin(h);
  return q;
};

/**
 * Orientation du téléphone selon le W3C : rotation intrinsèque Z-X'-Y'' (alpha, beta, gamma en degrés),
 * du repère de l'appareil (x droite, y haut, z hors de l'écran) vers le repère terrestre (z vers le haut).
 */
export const quatFromEuler = (alpha: number, beta: number, gamma: number): Quat =>
  quatMul(quatMul(axisQuat(2, alpha), axisQuat(0, beta)), axisQuat(1, gamma));

/**
 * Vitesse angulaire (deg/s) dans le repère de l'appareil entre deux orientations séparées de `dt` secondes.
 * Utilise la rotation relative conj(q0)·q1, par le chemin le plus court.
 */
export function angularVelocity(q0: Quat, q1: Quat, dt: number): Vec3 {
  let d = quatMul(quatConj(q0), q1);
  if (d[0] < 0) d = [-d[0], -d[1], -d[2], -d[3]];
  const s = Math.hypot(d[1], d[2], d[3]);
  if (s < 1e-12) return [0, 0, 0];
  const angle = 2 * Math.atan2(s, d[0]);
  const k = angle / s / dt / RAD;
  return [d[1] * k, d[2] * k, d[3] * k];
}

/** Direction du haut (gravité inversée) dans le repère de l'appareil : Ry(-γ)·Rx(-β)·e_z. */
export function upInBody(beta: number, gamma: number): Vec3 {
  const b = beta * RAD;
  const g = gamma * RAD;
  // Rx(-β)·e_z = (0, sin β, cos β)
  const y = Math.sin(b);
  const z = Math.cos(b);
  // Ry(-γ) : x' = x cos γ - z sin γ ; z' = x sin γ + z cos γ (avec x = 0)
  return [-z * Math.sin(g), y, z * Math.cos(g)];
}

/**
 * Accélération linéaire du téléphone : mesure brute moins la gravité attendue d'après l'orientation.
 * Le signe de la mesure brute varie selon les navigateurs (iOS l'inverse) ; il se déduit de la
 * gravité, qui domine toujours la mesure quand on ne secoue pas violemment le téléphone.
 */
export function linearAcceleration(raw: Vec3, up: Vec3): Vec3 {
  const k = raw[0] * up[0] + raw[1] * up[1] + raw[2] * up[2] >= 0 ? 1 : -1;
  return [k * raw[0] - G * up[0], k * raw[1] - G * up[1], k * raw[2] - G * up[2]];
}

export interface RawOrientation {
  alpha: number | null;
  beta: number | null;
  gamma: number | null;
}

export interface OrientationDeps {
  now?: () => number;
  angle?: () => number;
  /** Reçoit l'accélération linéaire quand elle est activée avec `setLinear(true)`. */
  onLinear?: (accel: Vec3, dt: number) => void;
}

/**
 * Source de mouvement fondée sur `deviceorientation` : les trois angles ont une définition sans
 * ambiguïté (contrairement à l'ordre des axes de `rotationRate` selon les navigateurs). La vitesse
 * angulaire et la gravité en sont déduites du même calcul, donc toujours cohérentes entre elles.
 */
export class OrientationSource {
  private prev: Quat | null = null;
  private last = 0;
  private listener: ((e: DeviceOrientationEvent) => void) | null = null;
  private readonly now: () => number;
  private readonly angle: () => number;
  private readonly onLinear?: (accel: Vec3, dt: number) => void;
  private up: Vec3 | null = null;
  private lastLinear: number | null = null;
  private linearListener: ((e: DeviceMotionEvent) => void) | null = null;
  /** Nombre d'événements reçus : permet de détecter un capteur muet. */
  received = 0;

  constructor(
    private readonly onSample: (s: MotionSample) => void,
    deps: OrientationDeps = {},
  ) {
    this.now = deps.now ?? (() => performance.now());
    this.angle = deps.angle ?? (() => screen.orientation?.angle ?? 0);
    this.onLinear = deps.onLinear;
  }

  /** Active ou coupe l'envoi de l'accélération linéaire (option embardée / pilonnement). */
  setLinear(enabled: boolean): void {
    if (enabled && !this.linearListener && this.onLinear) {
      this.lastLinear = null;
      this.linearListener = (e) => this.handleMotion(e.accelerationIncludingGravity);
      window.addEventListener("devicemotion", this.linearListener);
    } else if (!enabled && this.linearListener) {
      window.removeEventListener("devicemotion", this.linearListener);
      this.linearListener = null;
    }
  }

  handleMotion(a: { x: number | null; y: number | null; z: number | null } | null): void {
    if (!a || a.x === null || a.y === null || a.z === null || !this.up) return;
    const t = this.now();
    const prev = this.lastLinear;
    this.lastLinear = t;
    if (prev === null || t <= prev) return;
    const lin = linearAcceleration([a.x, a.y, a.z], this.up);
    this.onLinear?.(rotateForOrientation(lin, this.angle()), (t - prev) / 1000);
  }

  handle(ev: RawOrientation): void {
    if (ev.alpha === null || ev.beta === null || ev.gamma === null) return;
    this.received++;
    const t = this.now();
    const q = quatFromEuler(ev.alpha, ev.beta, ev.gamma);
    const prev = this.prev;
    const dt = (t - this.last) / 1000;
    this.prev = q;
    this.last = t;
    if (!prev || dt <= 0) return;
    const gyro = angularVelocity(prev, q, dt);
    const up = upInBody(ev.beta, ev.gamma);
    this.up = up;
    const accel: Vec3 = [up[0] * G, up[1] * G, up[2] * G];
    const a = this.angle();
    this.onSample({ gyro: rotateForOrientation(gyro, a), accel: rotateForOrientation(accel, a), dt });
  }

  start(): void {
    if (this.listener) return;
    this.prev = null;
    this.listener = (e) => this.handle(e);
    window.addEventListener("deviceorientation", this.listener);
  }

  stop(): void {
    if (this.listener) window.removeEventListener("deviceorientation", this.listener);
    this.listener = null;
    this.prev = null;
    this.setLinear(false);
  }
}
