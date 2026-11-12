import { AxisCalibrator, IDENTITY_MAP, W3C_MAP, applyMap, type AxisMap } from "./axis-calibration";
import type { MotionSample, Vec3 } from "./frames";

type RawVec = { x: number | null; y: number | null; z: number | null };

interface RawMotion {
  rotationRate: { alpha: number | null; beta: number | null; gamma: number | null } | null;
  accelerationIncludingGravity: RawVec | null;
  /** Accélération sans la gravité, quand le navigateur la fournit (Android). */
  acceleration?: RawVec | null;
  /** Période d'échantillonnage du capteur. */
  interval?: number | null;
}

/**
 * Période du capteur en secondes, d'après `DeviceMotionEvent.interval`. La spécification
 * l'exprime en millisecondes (Chrome) ; d'anciens Safari la donnaient en secondes.
 * Une valeur absente ou hors des cadences plausibles (1 à 100 ms) donne `null`.
 */
export function sampleInterval(interval: number | null | undefined): number | null {
  if (typeof interval !== "number" || !Number.isFinite(interval) || interval <= 0) return null;
  const s = interval >= 1 ? interval / 1000 : interval;
  return s >= 0.001 && s <= 0.1 ? s : null;
}

/**
 * Source de mouvement à privilégier. Sur iPhone, les angles d'orientation sont précis et
 * leurs axes sans ambiguïté. Sur Android, Chrome les calcule à partir d'angles d'Euler qui
 * deviennent instables quand le téléphone est tenu droit vers l'écran, et livre les
 * événements par paquets : la vitesse qu'on en déduit saute. Le gyroscope brut, lui,
 * respecte l'ordre d'axes du W3C et donne directement une vitesse propre.
 */
export const preferredSource = (ios = isIOS()): "orientation" | "motion" => (ios ? "orientation" : "motion");

export const isIOS = (ua = navigator.userAgent, platform = navigator.platform, touch = navigator.maxTouchPoints) =>
  /iPhone|iPad|iPod/.test(ua) || (platform === "MacIntel" && touch > 1);

/**
 * Ramène un événement `devicemotion` au repère W3C (x droite, y haut, z sortant de l'écran).
 * `rotationRate` : beta = autour de x, gamma = autour de y, alpha = autour de z.
 * iOS inverse le signe de l'accélération par rapport à la spécification.
 */
export function normalizeSample(
  ev: RawMotion,
  dt: number,
  opts: { invertAccel: boolean; map?: AxisMap },
): MotionSample {
  const r = ev.rotationRate;
  const a = ev.accelerationIncludingGravity;
  const k = opts.invertAccel ? -1 : 1;
  return {
    gyro: applyMap([r?.alpha ?? 0, r?.beta ?? 0, r?.gamma ?? 0], opts.map ?? W3C_MAP),
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
  /** Affectation d'axes de départ, avant que la mesure ne l'ait confirmée. */
  initialMap?: AxisMap;
  /** Appelée quand la mesure établit l'affectation d'axes du capteur. */
  onCalibrated?: (map: AxisMap) => void;
  /** L'affectation de départ est sûre (navigateur conforme au W3C) : pas de calibration. */
  trustMap?: boolean;
  /** Reçoit l'accélération sans gravité quand elle est activée avec `setLinear(true)`. */
  onLinear?: (accel: Vec3, dt: number) => void;
}

const MAP_KEY = "pwamote.axismap";

export function loadAxisMap(storage: Pick<Storage, "getItem"> = localStorage): AxisMap | null {
  try {
    const v = JSON.parse(storage.getItem(MAP_KEY) ?? "null") as AxisMap | null;
    if (v && Array.isArray(v.perm) && v.perm.length === 3 && (v.sign === 1 || v.sign === -1)) return v;
  } catch {
    /* stockage illisible */
  }
  return null;
}

export function saveAxisMap(map: AxisMap | null, storage: Pick<Storage, "setItem" | "removeItem"> = localStorage): void {
  try {
    if (map) storage.setItem(MAP_KEY, JSON.stringify(map));
    else storage.removeItem(MAP_KEY);
  } catch {
    /* navigation privée */
  }
}

/** Safari annonce les axes de rotation dans l'ordre de Core Motion, pas dans celui du W3C. */
export const defaultAxisMap = (ios = isIOS()): AxisMap => (ios ? IDENTITY_MAP : W3C_MAP);

export class MotionSource {
  private last: number | null = null;
  private readonly now: () => number;
  private readonly invertAccel: boolean;
  private readonly angle: () => number;
  private listener: ((e: DeviceMotionEvent) => void) | null = null;
  private map: AxisMap;
  private locked: boolean;
  private readonly calibrator = new AxisCalibrator();
  private readonly onCalibrated?: (map: AxisMap) => void;
  private count = 0;
  private readonly onLinear?: (accel: Vec3, dt: number) => void;
  private linear = false;
  private readonly trusted: boolean;
  /** Nombre d'événements portant une vitesse de rotation : permet de détecter un capteur muet. */
  received = 0;

  constructor(
    private readonly onSample: (s: MotionSample) => void,
    deps: MotionDeps = {},
  ) {
    this.now = deps.now ?? (() => performance.now());
    this.invertAccel = deps.invertAccel ?? isIOS();
    this.angle = deps.angle ?? (() => screen.orientation?.angle ?? 0);
    this.onCalibrated = deps.onCalibrated;
    this.onLinear = deps.onLinear;
    this.trusted = deps.trustMap ?? false;
    const saved = deps.initialMap || this.trusted ? null : loadAxisMap();
    this.map = deps.initialMap ?? saved ?? defaultAxisMap();
    // Une affectation déjà mesurée (ou sûre) n'est plus remise en cause.
    this.locked = saved !== null || this.trusted;
  }

  /** Active ou coupe l'envoi de l'accélération linéaire (option déplacements du téléphone). */
  setLinear(enabled: boolean): void {
    this.linear = enabled;
  }

  get axisMap(): AxisMap {
    return this.map;
  }

  /** Oublie l'affectation mesurée : elle sera redéterminée au prochain mouvement. */
  resetCalibration(): void {
    saveAxisMap(null);
    if (this.trusted) return;
    this.map = defaultAxisMap();
    this.locked = false;
    this.calibrator.reset();
  }

  handle(ev: RawMotion): void {
    const r0 = ev.rotationRate;
    if (r0 && (r0.alpha !== null || r0.beta !== null || r0.gamma !== null)) this.received++;
    const t = this.now();
    const prev = this.last;
    this.last = t;
    // La cadence du capteur prime : les événements arrivent parfois groupés, et diviser par
    // leur écart d'arrivée fausserait les vitesses (curseur saccadé qui se téléporte).
    const period = sampleInterval(ev.interval);
    if (period === null && prev === null) return;
    const dt = period ?? (t - prev!) / 1000;
    if (dt <= 0) return;
    if (!this.locked) {
      const r = ev.rotationRate;
      const a = ev.accelerationIncludingGravity;
      this.calibrator.push([r?.alpha ?? 0, r?.beta ?? 0, r?.gamma ?? 0], [a?.x ?? 0, a?.y ?? 0, a?.z ?? 0], dt);
      if (++this.count % 30 === 0) {
        const found = this.calibrator.best();
        if (found) {
          this.map = found;
          this.locked = true;
          saveAxisMap(found);
          this.onCalibrated?.(found);
        }
      }
    }
    const s = normalizeSample(ev, dt, { invertAccel: this.invertAccel, map: this.map });
    const angle = this.angle();
    this.onSample({ ...s, gyro: rotateForOrientation(s.gyro, angle), accel: rotateForOrientation(s.accel, angle) });
    const lin = ev.acceleration;
    if (this.linear && this.onLinear && lin && lin.x !== null && lin.y !== null && lin.z !== null) {
      const k = this.invertAccel ? -1 : 1;
      this.onLinear(rotateForOrientation([k * lin.x, k * lin.y, k * lin.z], angle), dt);
    }
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
  const g = globalThis as {
    DeviceMotionEvent?: { requestPermission?: () => Promise<string> };
    DeviceOrientationEvent?: { requestPermission?: () => Promise<string> };
  };
  const DME = g.DeviceMotionEvent;
  if (!DME) return "unsupported";
  try {
    // Les deux demandes partent dans le même geste ; l'orientation est facultative.
    const orientation = g.DeviceOrientationEvent?.requestPermission?.().catch(() => "denied");
    if (typeof DME.requestPermission === "function") {
      const motion = await DME.requestPermission();
      await orientation;
      return motion === "granted" ? "granted" : "denied";
    }
    await orientation;
    return "granted";
  } catch {
    return "denied";
  }
}
