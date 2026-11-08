// Miroir de `BinaryFrame` côté Rust (crates/pwamote-core/src/protocol.rs).

export type Vec3 = [number, number, number];

export interface MotionSample {
  /** Vitesse angulaire (x, y, z) en degrés par seconde. */
  gyro: Vec3;
  /** Accélération incluant la gravité, en m/s². */
  accel: Vec3;
  /** Secondes écoulées depuis l'échantillon précédent. */
  dt: number;
}

const KIND_MOTION = 0x01;
const KIND_TRACKPAD = 0x02;
const KIND_SCROLL = 0x03;

function pack(kind: number, values: number[]): ArrayBuffer {
  const buf = new ArrayBuffer(1 + values.length * 4);
  const view = new DataView(buf);
  view.setUint8(0, kind);
  values.forEach((v, i) => view.setFloat32(1 + i * 4, v, true));
  return buf;
}

export const encodeMotion = (s: MotionSample) => pack(KIND_MOTION, [...s.gyro, ...s.accel, s.dt]);
export const encodeTrackpad = (dx: number, dy: number) => pack(KIND_TRACKPAD, [dx, dy]);
export const encodeScroll = (dx: number, dy: number) => pack(KIND_SCROLL, [dx, dy]);
