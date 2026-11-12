import { diffText } from "./text-diff";

/** Au-delà, on repart d'un champ vide : le PC a déjà reçu tout le texte. */
export const MAX_FIELD = 300;

export const isAndroid = (ua = navigator.userAgent) => /Android/i.test(ua);

export interface TextOut {
  backspace(count: number): void;
  text(text: string): void;
}

/**
 * Reporte sur le PC ce qui change dans le champ de saisie du téléphone.
 *
 * Sur Android, Gboard et la plupart des claviers « composent » chaque mot latin jusqu'à
 * l'espace ou à la validation : attendre la fin de la composition, comme sur iPhone, ne
 * laisserait rien passer pendant la frappe, et Entrée effaçait le champ avant que le mot
 * ne parte. Là-bas, on envoie donc à chaque modification ; le calcul de différence
 * corrige ensuite le mot si l'autocorrection le remplace.
 */
export class TextSync {
  private sent = "";
  private composing = false;

  constructor(
    private readonly out: TextOut,
    private readonly opts: { waitForComposition: boolean },
  ) {}

  compositionStart(): void {
    this.composing = true;
  }

  compositionEnd(value: string): boolean {
    this.composing = false;
    return this.input(value);
  }

  /** Nouveau contenu du champ. Renvoie `true` quand il faut vider le champ (trop long). */
  input(value: string): boolean {
    if (this.composing && this.opts.waitForComposition) return false;
    return this.flush(value);
  }

  /** Envoie tout de suite ce qui manque, composition en cours ou non (Entrée, touches). */
  flush(value: string): boolean {
    const { backspaces, insert } = diffText(this.sent, value);
    this.sent = value;
    if (backspaces > 0) this.out.backspace(backspaces);
    if (insert) this.out.text(insert);
    return this.sent.length > MAX_FIELD;
  }

  /** Le champ vient d'être vidé : le PC a tout reçu. */
  reset(): void {
    this.sent = "";
  }
}
