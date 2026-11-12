/**
 * Curseur (input range) dans une page qui défile. Sur Android, un curseur natif prend la
 * valeur dès qu'on le touche, même quand on voulait faire défiler les réglages. On suit
 * la règle des listes Material : le geste ne devient un réglage que s'il part à
 * l'horizontale ; parti à la verticale, il reste un défilement. Un simple toucher ne
 * change rien, pour qu'arrêter un défilement lancé ne dérègle pas un curseur.
 */
export type SliderIntent = "idle" | "pending" | "drag" | "scroll";

/** Distance (px) avant de décider entre réglage et défilement. */
const SLOP = 8;

export class SliderGesture {
  private start: { x: number; y: number } | null = null;
  private intent: SliderIntent = "idle";

  down(x: number, y: number): void {
    this.start = { x, y };
    this.intent = "pending";
  }

  move(x: number, y: number): SliderIntent {
    if (this.intent === "pending" && this.start) {
      const dx = Math.abs(x - this.start.x);
      const dy = Math.abs(y - this.start.y);
      if (dx > SLOP && dx > dy) this.intent = "drag";
      else if (dy > SLOP && dy >= dx) this.intent = "scroll";
    }
    return this.intent;
  }

  /** Fin du toucher : renvoie ce qu'il était devenu. */
  up(): SliderIntent {
    const was = this.intent;
    this.start = null;
    this.intent = "idle";
    return was;
  }
}

export interface Track {
  left: number;
  width: number;
  /** Marge de chaque côté où le centre du bouton ne va pas (moitié de sa largeur). */
  inset: number;
}

/** Valeur sous le doigt, bornée et arrondie au pas (comme le ferait le curseur natif). */
export function valueAt(x: number, track: Track, min: number, max: number, step: number): number {
  const usable = Math.max(1, track.width - 2 * track.inset);
  const ratio = Math.min(1, Math.max(0, (x - track.left - track.inset) / usable));
  const raw = min + ratio * (max - min);
  const steps = step > 0 ? Math.round((raw - min) / step) : 0;
  const snapped = step > 0 ? min + steps * step : raw;
  const decimals = (String(step).split(".")[1] ?? "").length + (String(min).split(".")[1] ?? "").length;
  return Math.min(max, Math.max(min, Number(snapped.toFixed(Math.min(10, decimals)))));
}
