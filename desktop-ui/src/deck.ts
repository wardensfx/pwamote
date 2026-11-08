// Logique pure de l'éditeur d'actions (sans DOM), testée à part.
import type { ActionButton } from "../../client/src/generated/ActionButton";
import type { ActionInput } from "../../client/src/generated/ActionInput";
import type { ActionStep } from "../../client/src/generated/ActionStep";
import type { Deck } from "../../client/src/generated/Deck";
import type { DeckPage } from "../../client/src/generated/DeckPage";
import type { InputKind } from "../../client/src/generated/InputKind";
import type { Key } from "../../client/src/generated/Key";
import type { Modifier } from "../../client/src/generated/Modifier";

export type { ActionButton, ActionInput, ActionStep, Deck, DeckPage, InputKind, Key, Modifier };

/** Limites de la grille, identiques à celles du serveur (pwamote-core/actions.rs). */
export const GRID = { minColumns: 2, maxColumns: 5, minRows: 1, maxRows: 6, maxPages: 24 };

export function uid(): string {
  const bytes = new Uint8Array(6);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (b) => "abcdefghijklmnopqrstuvwxyz0123456789"[b % 36]).join("");
}

export function newPage(index: number, id = uid()): DeckPage {
  return { id, name: `Page ${index + 1}`, columns: 3, rows: 2, cells: Array(6).fill(null) };
}

export function newButton(id = uid()): ActionButton {
  return { id, label: "Nouvelle action", icon: "", color: "glass", inputs: [], steps: [{ kind: "keys", key: "space", modifiers: [] }] };
}

/** Plus petite grille qui garde tous les boutons à leur place. */
export function occupiedExtent(page: DeckPage): { columns: number; rows: number } {
  let columns = GRID.minColumns;
  let rows = GRID.minRows;
  page.cells.forEach((c, i) => {
    if (!c) return;
    columns = Math.max(columns, (i % page.columns) + 1);
    rows = Math.max(rows, Math.floor(i / page.columns) + 1);
  });
  return { columns, rows };
}

/** Change la taille de la grille en gardant chaque bouton sur sa ligne et sa colonne. */
export function resizePage(page: DeckPage, columns: number, rows: number): DeckPage {
  const min = occupiedExtent(page);
  const c = Math.min(GRID.maxColumns, Math.max(min.columns, columns));
  const r = Math.min(GRID.maxRows, Math.max(min.rows, rows));
  const cells: (ActionButton | null)[] = Array(c * r).fill(null);
  page.cells.forEach((cell, i) => {
    if (cell) cells[Math.floor(i / page.columns) * c + (i % page.columns)] = cell;
  });
  return { ...page, columns: c, rows: r, cells };
}

/** Échange deux cases (glisser-déposer). */
export function swapCells(page: DeckPage, a: number, b: number): DeckPage {
  const cells = [...page.cells];
  [cells[a], cells[b]] = [cells[b] ?? null, cells[a] ?? null];
  return { ...page, cells };
}

export function move<T>(list: T[], from: number, to: number): T[] {
  if (to < 0 || to >= list.length) return list;
  const out = [...list];
  const [x] = out.splice(from, 1);
  out.splice(to, 0, x!);
  return out;
}

// ---------- Touches ----------

export const NAMED_KEYS: [Key, string][] = [
  ["escape", "Échap"], ["tab", "Tab"], ["enter", "Entrée"], ["backspace", "Retour arrière"], ["delete", "Suppr"],
  ["space", "Espace"], ["arrow_up", "Flèche haut"], ["arrow_down", "Flèche bas"], ["arrow_left", "Flèche gauche"],
  ["arrow_right", "Flèche droite"], ["home", "Début"], ["end", "Fin"], ["page_up", "Page préc."], ["page_down", "Page suiv."],
  ["insert", "Inser"], ["f1", "F1"], ["f2", "F2"], ["f3", "F3"], ["f4", "F4"], ["f5", "F5"], ["f6", "F6"], ["f7", "F7"],
  ["f8", "F8"], ["f9", "F9"], ["f10", "F10"], ["f11", "F11"], ["f12", "F12"], ["print_screen", "Impr. écran"],
  ["volume_up", "Volume +"], ["volume_down", "Volume −"], ["volume_mute", "Muet"], ["media_play_pause", "Lecture / pause"],
  ["media_next", "Piste suivante"], ["media_previous", "Piste précédente"],
];
export const CHAR_KEYS: Key[] = [..."abcdefghijklmnopqrstuvwxyz0123456789"] as Key[];

const MOD_LABEL: Record<Modifier, string> = { ctrl: "Ctrl", shift: "Maj", alt: "Alt", meta: "Win" };
export const MODIFIERS: Modifier[] = ["ctrl", "shift", "alt", "meta"];

export function modifierLabel(m: Modifier): string {
  return MOD_LABEL[m];
}

export function keyLabel(key: Key): string {
  const named = NAMED_KEYS.find(([k]) => k === key);
  return named ? named[1] : String(key).toUpperCase();
}

export function shortcutLabel(key: Key, modifiers: Modifier[]): string {
  return [...MODIFIERS.filter((m) => modifiers.includes(m)).map(modifierLabel), keyLabel(key)].join(" + ");
}

const CODE_KEYS: Record<string, Key> = {
  Escape: "escape", Tab: "tab", Enter: "enter", NumpadEnter: "enter", Backspace: "backspace", Delete: "delete", Space: "space",
  ArrowUp: "arrow_up", ArrowDown: "arrow_down", ArrowLeft: "arrow_left", ArrowRight: "arrow_right", Home: "home", End: "end",
  PageUp: "page_up", PageDown: "page_down", Insert: "insert", PrintScreen: "print_screen", AudioVolumeUp: "volume_up",
  AudioVolumeDown: "volume_down", AudioVolumeMute: "volume_mute", MediaPlayPause: "media_play_pause",
  MediaTrackNext: "media_next", MediaTrackPrevious: "media_previous",
};

/**
 * Touche capturée au clavier du PC. On lit `code` (position physique) : sur un clavier AZERTY,
 * la touche « A » a le code `KeyQ`, mais c'est bien un « a » que Windows tape, d'où `key` pour les lettres.
 */
export function keyFromEvent(e: { key: string; code: string }): Key | null {
  if (CODE_KEYS[e.code]) return CODE_KEYS[e.code]!;
  const f = /^F([1-9]|1[0-2])$/.exec(e.code);
  if (f) return `f${f[1]}` as Key;
  if (/^(Shift|Control|Alt|Meta|OS)(Left|Right)?$/.test(e.code) || /^(Shift|Control|Alt|Meta)$/.test(e.key)) return null;
  const k = e.key.length === 1 ? e.key.toLowerCase() : "";
  if (/^[a-z]$/.test(k)) return k as Key;
  const digit = /^(?:Digit|Numpad)(\d)$/.exec(e.code);
  if (digit) return digit[1] as Key;
  return null;
}

export function modifiersFromEvent(e: { ctrlKey: boolean; shiftKey: boolean; altKey: boolean; metaKey: boolean }): Modifier[] {
  return MODIFIERS.filter((m) => ({ ctrl: e.ctrlKey, shift: e.shiftKey, alt: e.altKey, meta: e.metaKey })[m]);
}

// ---------- Étapes et champs ----------

export const STEP_KINDS: [ActionStep["kind"], string][] = [
  ["keys", "Touche ou raccourci"],
  ["text", "Taper du texte"],
  ["run", "Lancer un programme"],
  ["open", "Ouvrir une adresse ou un fichier"],
  ["delay", "Pause"],
];

export function newStep(kind: ActionStep["kind"]): ActionStep {
  switch (kind) {
    case "keys":
      return { kind, key: "space", modifiers: [] };
    case "text":
      return { kind, text: "" };
    case "run":
      return { kind, program: "", args: [], cwd: "", hidden: false };
    case "open":
      return { kind, target: "https://" };
    case "delay":
      return { kind, ms: 200 };
  }
}

export const INPUT_KINDS: [InputKind["type"], string][] = [
  ["choice", "Liste de choix"],
  ["slider", "Curseur"],
  ["number", "Nombre"],
  ["text", "Texte libre"],
  ["toggle", "Interrupteur"],
];

export function newInputKind(type: InputKind["type"]): InputKind {
  switch (type) {
    case "choice":
      return { type, options: [{ label: "Option 1", value: "1" }, { label: "Option 2", value: "2" }], default: "1" };
    case "slider":
      return { type, min: 0, max: 100, step: 1, default: 50, unit: "" };
    case "number":
      return { type, min: null, max: null, default: 0, integer: true };
    case "text":
      return { type, default: "", placeholder: "" };
    case "toggle":
      return { type, default: false, on: "true", off: "false" };
  }
}

export function newInput(taken: string[]): ActionInput {
  let n = taken.length + 1;
  while (taken.includes(`champ${n}`)) n++;
  return { id: `champ${n}`, label: `Champ ${n}`, kind: newInputKind("text") };
}

/** Nom de champ utilisable dans `{nom}` : lettres sans accent, chiffres, `_`. */
export function toFieldId(text: string): string {
  return text
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9_]+/g, "_")
    .replace(/^_+|_+$/g, "")
    .slice(0, 32);
}

/** Problèmes qui empêcheraient une action de marcher, à montrer avant d'enregistrer. */
export function buttonProblems(b: ActionButton): string[] {
  const out: string[] = [];
  if (b.steps.length === 0) out.push("Aucune étape : le bouton ne fera rien.");
  if (b.steps.some((s) => s.kind === "run" && !s.program.trim())) out.push("Une étape « Lancer » n'a pas de programme.");
  if (b.steps.some((s) => s.kind === "open" && !s.target.trim())) out.push("Une étape « Ouvrir » n'a pas de cible.");
  const ids = b.inputs.map((i) => i.id);
  if (ids.some((id) => !/^[a-zA-Z0-9_]+$/.test(id))) out.push("Un nom de champ contient des caractères interdits (lettres, chiffres et _ seulement).");
  if (new Set(ids).size !== ids.length) out.push("Deux champs portent le même nom.");
  if (b.steps.some((s) => s.kind === "open" && /^\s*\{[a-zA-Z0-9_]+\}/.test(s.target)))
    out.push("Une cible « Ouvrir » commence par un champ : le téléphone pourrait ouvrir n'importe quel fichier. Préférez une adresse fixe avec {champ:url}.");
  return out;
}

/** Champs utilisés dans les étapes mais non déclarés (souvent une faute de frappe). */
export function unknownFields(b: ActionButton): string[] {
  const texts = b.steps.flatMap((s) => (s.kind === "text" ? [s.text] : s.kind === "open" ? [s.target] : s.kind === "run" ? s.args : []));
  const used = new Set<string>();
  for (const t of texts) for (const m of t.replace(/\{\{|\}\}/g, "").matchAll(/\{([a-zA-Z0-9_]+)(?::url)?\}/g)) used.add(m[1]!);
  const declared = new Set(b.inputs.map((i) => i.id));
  return [...used].filter((u) => !declared.has(u));
}

export function stepSummary(s: ActionStep): string {
  switch (s.kind) {
    case "keys":
      return shortcutLabel(s.key, s.modifiers);
    case "text":
      return s.text ? `« ${s.text.length > 30 ? s.text.slice(0, 30) + "…" : s.text} »` : "texte vide";
    case "run":
      return s.program || "programme à choisir";
    case "open":
      return s.target || "cible à choisir";
    case "delay":
      return `${s.ms} ms`;
  }
}

/** Copie profonde, pour éditer un brouillon sans toucher à la version enregistrée. */
export function clone<T>(v: T): T {
  return JSON.parse(JSON.stringify(v));
}
