import { describe, expect, it } from "vitest";
import { encodeMotion, encodeScroll, encodeTrackpad } from "./frames";

const f32 = (b: ArrayBuffer, offset: number) => new DataView(b).getFloat32(offset, true);

describe("trames binaires", () => {
  it("encode un échantillon de mouvement comme le serveur l'attend (29 octets, little-endian)", () => {
    const b = encodeMotion({ gyro: [1, 2, 3], accel: [4, 5, 6], dt: 0.016 });
    expect(b.byteLength).toBe(29);
    expect(new Uint8Array(b)[0]).toBe(0x01);
    expect([1, 5, 9, 13, 17, 21].map((o) => f32(b, o))).toEqual([1, 2, 3, 4, 5, 6]);
    expect(f32(b, 25)).toBeCloseTo(0.016, 6);
  });

  it("encode trackpad et défilement sur 9 octets", () => {
    const t = encodeTrackpad(1.5, -2);
    expect(t.byteLength).toBe(9);
    expect(new Uint8Array(t)[0]).toBe(0x02);
    expect([f32(t, 1), f32(t, 5)]).toEqual([1.5, -2]);
    expect(new Uint8Array(encodeScroll(0, 3))[0]).toBe(0x03);
  });
});
