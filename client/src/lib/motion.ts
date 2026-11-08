import type { MotionSample, Vec3 } from "./frames";

interface RawMotion {
  rotationRate: { alpha: number | null; beta: number | null; gamma: number | null } | null;
  accelerationIncludingGravity: { x: number | null; y: number | null; z: number | null } | null;
}

export const isIOS = (ua = navigator.userAgent, platform = navigator.platform, touch = navigator.maxTouchPoints) =>
  /iPhone|iPad|iPod/.test(ua) || (platform === "MacIntel" && touch > 1);

/**
 * Ramène un événement `devicemotion` au repère W3C (x droite, y haut, z sortant de l'écran).
 * `rotationRate` : beta = autour de x, gamma = autour de y, alpha = autour de z.
 * iOS inverse le signe de l'accélération par rapport à la spécification.
 */
export function normalizeSample(ev: RawMotion, dt: number, opts: { invertAccel: boolean }): MotionSample {
  const r = ev.rotationRate;
  const a = ev.accelerationIncludingGravity;
  const k = opts.invertAccel ? -1 : 1;
  return {
    gyro: [r?.beta ?? 0, r?.gamma ?? 0, r?.alpha ?? 0],
    accel: [k * (a?.x ?? 0), k * (a?.y ?? 0), k * (a?.z ?? 0)],
    dt,
  };
}

/** Réaligne x/y quand l'écran est tourné (en degrés : 0, 90, 180, 270). */
export function rotateForOrientation(v: Vec3, angle: number): Vec3 {
  const [x, y, z] = v;
  switch (((angle % 360) + 360) % 360) {
    case 90:
      return [-y, x, z];
    case 180:
      return [-x, -y, z];
    case 270:
      return [y, -x, z];
    default:
      return [x, y, z];
  }
}

export interface MotionDeps {
  now?: () => number;
  invertAccel?: boolean;
  angle?: () => number;
}

export class MotionSource {
  private last: number | null = null;
  private readonly now: () => number;
  private readonly invertAccel: boolean;
  private readonly angle: () => number;
  private listener: ((e: DeviceMotionEvent) => void) | null = null;

  constructor(
    private readonly onSample: (s: MotionSample) => void,
    deps: MotionDeps = {},
  ) {
    this.now = deps.now ?? (() => performance.now());
    this.invertAccel = deps.invertAccel ?? isIOS();
    this.angle = deps.angle ?? (() => screen.orientation?.angle ?? 0);
  }

  handle(ev: RawMotion): void {
    const t = this.now();
    const prev = this.last;
    this.last = t;
    if (prev === null) return;
    const dt = (t - prev) / 1000;
    if (dt <= 0) return;
    const s = normalizeSample(ev, dt, { invertAccel: this.invertAccel });
    const angle = this.angle();
    this.onSample({ ...s, gyro: rotateForOrientation(s.gyro, angle), accel: rotateForOrientation(s.accel, angle) });
  }

  start(): void {
    if (this.listener) return;
    this.last = null;
    this.listener = (e) => this.handle(e);
    window.addEventListener("devicemotion", this.listener);
  }

  stop(): void {
    if (this.listener) window.removeEventListener("devicemotion", this.listener);
    this.listener = null;
    this.last = null;
  }
}

export type MotionPermission = "granted" | "denied" | "unsupported";

/** Sur iOS, l'autorisation doit être demandée depuis un geste de l'utilisateur. */
export async function requestMotionPermission(): Promise<MotionPermission> {
  const DME = (globalThis as { DeviceMotionEvent?: { requestPermission?: () => Promise<string> } }).DeviceMotionEvent;
  if (!DME) return "unsupported";
  if (typeof DME.requestPermission === "function") {
    try {
      return (await DME.requestPermission()) === "granted" ? "granted" : "denied";
    } catch {
      return "denied";
    }
  }
  return "granted";
}
