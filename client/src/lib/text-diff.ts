/**
 * Différence entre ce qui a déjà été envoyé au PC et le contenu actuel du
 * champ : combien de caractères effacer, puis quel texte taper. Le champ
 * garde son contenu, ce qui permet à l'autocorrection de remplacer un mot
 * déjà saisi (effacements puis nouveau mot) et d'avoir des majuscules justes.
 */
export interface TextEdit {
  backspaces: number;
  insert: string;
}

export function diffText(previous: string, next: string): TextEdit {
  const a = Array.from(previous);
  const b = Array.from(next);
  let common = 0;
  while (common < a.length && common < b.length && a[common] === b[common]) common++;
  return { backspaces: a.length - common, insert: b.slice(common).join("") };
}
