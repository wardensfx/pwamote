import type { PointerMode } from "../generated/PointerMode";

export interface PadSink {
  clutch(engaged: boolean): void;
  trackpad(dx: number, dy: number): void;
  scroll(dx: number, dy: number): void;
  click(): void;
}

const TAP_MAX_MS = 220;
const TAP_MAX_MOVE = 8;
/**
 * Un doigt posé qui glisse verticalement devient un défilement s'il parcourt cette distance (px)
 * dans la fenêtre de temps ci-dessous : une dérive lente du pouce pendant qu'on vise ne compte pas.
 */
const THUMB_SCROLL_START = 40;
const THUMB_SCROLL_WINDOW_MS = 250;
/** Le glissement doit être nettement plus vertical qu'horizontal pour défiler. */
const THUMB_SCROLL_RATIO = 2;
/** Un tap en mode gyro est plus court qu'en trackpad : un pouce posé pour viser ne doit pas cliquer. */
const AIR_TAP_MAX_MS = 170;

/**
 * Interprète les doigts posés sur la surface centrale.
 *
 * - un doigt posé : embrayage du gyroscope (`clutch`) ;
 * - un tap bref clique, et deux taps rapprochés font un double clic (modes trackpad et gyro pouce) ;
 * - en mode trackpad : un doigt déplace le curseur ;
 * - deux doigts : défilement, dans tous les modes ;
 * - en mode « gyro pouce », glisser le pouce vers le haut ou le bas défile, le curseur restant en place.
 */
export class PadGestures {
  private pointers = new Map<number, { x: number; y: number }>();
  private startedAt = 0;
  private lastT = 0;
  private travelled = 0;
  private sawSecondFinger = false;
  private origin = { x: 0, y: 0 };
  private thumbScrolling = false;
  /** Positions récentes du doigt, pour mesurer la distance parcourue dans la fenêtre glissante. */
  private trail: { t: number; x: number; y: number }[] = [];

  constructor(
    private readonly sink: PadSink,
    private readonly mode: () => PointerMode,
  ) {}

  down(id: number, x: number, y: number, t: number): void {
    if (this.pointers.size === 0) {
      this.startedAt = t;
      this.lastT = t;
      this.travelled = 0;
      this.sawSecondFinger = false;
      this.thumbScrolling = false;
      this.trail = [{ t, x, y }];
      this.origin = { x, y };
      this.sink.clutch(true);
    } else {
      this.sawSecondFinger = true;
    }
    this.pointers.set(id, { x, y });
  }

  move(id: number, x: number, y: number, t: number = this.lastT + 16): void {
    this.lastT = t;
    const p = this.pointers.get(id);
    if (!p) return;
    const dx = x - p.x;
    const dy = y - p.y;
    p.x = x;
    p.y = y;
    this.travelled += Math.hypot(dx, dy);

    if (this.pointers.size >= 2) {
      // Chaque doigt contribue pour moitié : le défilement suit le milieu des deux.
      this.sink.scroll(dx / this.pointers.size, dy / this.pointers.size);
    } else if (this.mode() === "trackpad") {
      this.sink.trackpad(dx, dy);
    } else if (this.mode() === "air_hold") {
      if (!this.thumbScrolling) {
        this.trail.push({ t, x, y });
        while (this.trail.length > 1 && t - this.trail[0]!.t > THUMB_SCROLL_WINDOW_MS) this.trail.shift();
        const from = this.trail[0]!;
        const vy = Math.abs(y - from.y);
        const vx = Math.abs(x - from.x);
        if (vy >= THUMB_SCROLL_START && vy > THUMB_SCROLL_RATIO * vx) {
          this.thumbScrolling = true;
          // Le curseur reste là où il est pendant que la page défile.
          this.sink.clutch(false);
        }
      }
      if (this.thumbScrolling) this.sink.scroll(0, dy);
    }
  }

  up(id: number, t: number): void {
    if (!this.pointers.delete(id)) return;
    if (this.pointers.size > 0) return;
    const maxMs = this.mode() === "trackpad" ? TAP_MAX_MS : AIR_TAP_MAX_MS;
    const isTap = t - this.startedAt <= maxMs && this.travelled <= TAP_MAX_MOVE && !this.sawSecondFinger;
    // Deux taps rapprochés donnent deux clics : Windows en fait un double clic.
    if (isTap && this.mode() !== "air_always") this.sink.click();
    this.sink.clutch(false);
  }

  cancel(): void {
    if (this.pointers.size > 0) this.sink.clutch(false);
    this.pointers.clear();
  }
}
