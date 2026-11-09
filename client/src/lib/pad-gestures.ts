import type { PointerMode } from "../generated/PointerMode";

export interface PadSink {
  clutch(engaged: boolean): void;
  trackpad(dx: number, dy: number): void;
  scroll(dx: number, dy: number): void;
  click(): void;
  /** Bouton gauche maintenu (gyro continu : appui long pour glisser-déposer). */
  press(down: boolean): void;
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
 * En gyro continu, un doigt resté posé plus longtemps enfonce le bouton gauche (glisser-déposer).
 * Avant ce délai, on attend de savoir si c'est un tap (clic) ou un balayage (défilement).
 */
const AIR_PRESS_AFTER_MS = 250;

/**
 * Interprète les doigts posés sur la surface centrale.
 *
 * - un doigt posé : embrayage du gyroscope (`clutch`) ;
 * - un tap bref clique, et deux taps rapprochés font un double clic (modes trackpad et gyro pouce) ;
 * - en mode trackpad : un doigt déplace le curseur ;
 * - deux doigts : défilement, dans tous les modes ;
 * - en mode « gyro pouce », glisser le pouce vers le haut ou le bas défile, le curseur restant en place ;
 * - en mode « gyro continu », la surface est le bouton gauche : un tap clique, un appui maintenu
 *   enfonce le bouton jusqu'au lever du doigt, et un balayage vertical défile.
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
  private pressTimer: ReturnType<typeof setTimeout> | null = null;
  private pressed = false;

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
      this.pressed = false;
      this.sink.clutch(true);
      if (this.mode() === "air_always") {
        this.pressTimer = setTimeout(() => {
          this.pressTimer = null;
          this.pressed = true;
          this.sink.press(true);
        }, AIR_PRESS_AFTER_MS);
      }
    } else {
      this.sawSecondFinger = true;
      this.clearPressTimer();
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
      if (!this.thumbScrolling && this.detectThumbScroll(x, y, t)) {
        this.thumbScrolling = true;
        // Le curseur reste là où il est pendant que la page défile.
        this.sink.clutch(false);
      }
      if (this.thumbScrolling) this.sink.scroll(0, dy);
    } else if (this.mode() === "air_always" && !this.pressed) {
      if (!this.thumbScrolling && this.detectThumbScroll(x, y, t)) {
        this.thumbScrolling = true;
        this.clearPressTimer();
      }
      if (this.thumbScrolling) this.sink.scroll(0, dy);
    }
  }

  /** Vrai si le doigt vient de parcourir assez vite une distance nettement verticale. */
  private detectThumbScroll(x: number, y: number, t: number): boolean {
    this.trail.push({ t, x, y });
    while (this.trail.length > 1 && t - this.trail[0]!.t > THUMB_SCROLL_WINDOW_MS) this.trail.shift();
    const from = this.trail[0]!;
    const vy = Math.abs(y - from.y);
    const vx = Math.abs(x - from.x);
    return vy >= THUMB_SCROLL_START && vy > THUMB_SCROLL_RATIO * vx;
  }

  private clearPressTimer(): void {
    if (this.pressTimer) clearTimeout(this.pressTimer);
    this.pressTimer = null;
  }

  up(id: number, t: number): void {
    if (!this.pointers.delete(id)) return;
    if (this.pointers.size > 0) return;
    if (this.mode() === "air_always") {
      this.clearPressTimer();
      // C'est un bouton : tout appui bref qui n'a pas défilé clique, même si le pouce a un peu bougé.
      if (this.pressed) this.sink.press(false);
      else if (!this.thumbScrolling && !this.sawSecondFinger) this.sink.click();
      this.pressed = false;
      this.sink.clutch(false);
      return;
    }
    const maxMs = this.mode() === "trackpad" ? TAP_MAX_MS : AIR_TAP_MAX_MS;
    const isTap = t - this.startedAt <= maxMs && this.travelled <= TAP_MAX_MOVE && !this.sawSecondFinger;
    // Deux taps rapprochés donnent deux clics : Windows en fait un double clic.
    if (isTap) this.sink.click();
    this.sink.clutch(false);
  }

  cancel(): void {
    this.clearPressTimer();
    if (this.pressed) this.sink.press(false);
    this.pressed = false;
    if (this.pointers.size > 0) this.sink.clutch(false);
    this.pointers.clear();
  }
}
