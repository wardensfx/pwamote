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
 * Interprète les doigts posés sur la surface centrale.
 *
 * - un doigt posé : embrayage du gyroscope (`clutch`) ;
 * - en mode trackpad : un doigt déplace le curseur, un tap clique ;
 * - deux doigts : défilement, dans tous les modes.
 */
export class PadGestures {
  private pointers = new Map<number, { x: number; y: number }>();
  private startedAt = 0;
  private travelled = 0;
  private sawSecondFinger = false;

  constructor(
    private readonly sink: PadSink,
    private readonly mode: () => PointerMode,
  ) {}

  down(id: number, x: number, y: number, t: number): void {
    if (this.pointers.size === 0) {
      this.startedAt = t;
      this.travelled = 0;
      this.sawSecondFinger = false;
      this.sink.clutch(true);
    } else {
      this.sawSecondFinger = true;
    }
    this.pointers.set(id, { x, y });
  }

  move(id: number, x: number, y: number): void {
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
    }
  }

  up(id: number, t: number): void {
    if (!this.pointers.delete(id)) return;
    if (this.pointers.size > 0) return;
    const isTap = t - this.startedAt <= TAP_MAX_MS && this.travelled <= TAP_MAX_MOVE && !this.sawSecondFinger;
    if (isTap && this.mode() === "trackpad") this.sink.click();
    this.sink.clutch(false);
  }

  cancel(): void {
    if (this.pointers.size > 0) this.sink.clutch(false);
    this.pointers.clear();
  }
}
