import type { Modifier } from "../generated/Modifier";

export type ModState = "off" | "once" | "locked";

/**
 * Modificateurs « collants » : un appui arme le modificateur pour la touche
 * suivante, un second appui le verrouille, un troisième le relâche.
 */
export class StickyModifiers {
  private state = new Map<Modifier, ModState>();

  constructor(private sticky: boolean) {}

  setSticky(sticky: boolean): void {
    this.sticky = sticky;
    if (!sticky) this.state.clear();
  }

  get(m: Modifier): ModState {
    return this.state.get(m) ?? "off";
  }

  /** Appui sur la pastille d'un modificateur. */
  press(m: Modifier): ModState {
    const next: ModState = this.get(m) === "off" ? "once" : this.get(m) === "once" ? "locked" : "off";
    this.state.set(m, next);
    return next;
  }

  /** Modificateurs à appliquer à la prochaine touche. */
  active(): Modifier[] {
    return [...this.state].filter(([, s]) => s !== "off").map(([m]) => m);
  }

  /** À appeler après chaque touche : libère les modificateurs « une fois ». */
  consume(): Modifier[] {
    const released: Modifier[] = [];
    for (const [m, s] of this.state) {
      if (s === "once") {
        this.state.set(m, "off");
        released.push(m);
      }
    }
    return released;
  }

  clear(): void {
    this.state.clear();
  }
}
