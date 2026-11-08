import type { ActionInput } from "../generated/ActionInput";
import type { InputValue } from "../generated/InputValue";

export type Values = Record<string, InputValue>;

/** Valeur par défaut d'un champ, telle que déclarée sur le PC. */
export function defaultValue(input: ActionInput): InputValue {
  const k = input.kind;
  switch (k.type) {
    case "choice":
      return k.options.some((o) => o.value === k.default) ? k.default : (k.options[0]?.value ?? "");
    case "slider":
    case "number":
      return k.default;
    case "text":
      return k.default;
    case "toggle":
      return k.default;
  }
}

/** La valeur est-elle acceptable pour ce champ ? (Le PC revérifie de toute façon.) */
export function isValid(input: ActionInput, value: InputValue | undefined): boolean {
  const k = input.kind;
  switch (k.type) {
    case "choice":
      return typeof value === "string" && k.options.some((o) => o.value === value);
    case "slider":
      return typeof value === "number" && Number.isFinite(value) && value >= k.min && value <= k.max;
    case "number": {
      const n = typeof value === "string" ? parseNumber(value) : value;
      if (typeof n !== "number" || !Number.isFinite(n)) return false;
      if (k.integer && !Number.isInteger(n)) return false;
      return (k.min === null || n >= k.min) && (k.max === null || n <= k.max);
    }
    case "text":
      return typeof value === "string" && value.length <= 1000;
    case "toggle":
      return typeof value === "boolean";
  }
}

/** Accepte la virgule décimale du clavier français. */
export function parseNumber(text: string): number | null {
  const t = text.trim().replace(",", ".");
  if (t === "" || !/^[-+]?(\d+\.?\d*|\.\d+)$/.test(t)) return null;
  return Number(t);
}

/** Valeurs de départ du formulaire : les dernières utilisées si elles sont encore valides. */
export function initialValues(inputs: ActionInput[], remembered: Values | undefined): Values {
  const out: Values = {};
  for (const input of inputs) {
    const prev = remembered?.[input.id];
    out[input.id] = prev !== undefined && isValid(input, prev) ? prev : defaultValue(input);
  }
  return out;
}

/** Valeurs prêtes à envoyer : les nombres saisis en texte deviennent des nombres. */
export function finalValues(inputs: ActionInput[], values: Values): Values {
  const out: Values = {};
  for (const input of inputs) {
    const v = values[input.id];
    if (v === undefined) continue;
    out[input.id] = input.kind.type === "number" && typeof v === "string" ? (parseNumber(v) ?? v) : v;
  }
  return out;
}

/** Nombre affiché sans bruit flottant, avec la précision du pas. */
export function formatStep(value: number, step: number): string {
  const decimals = Math.min(6, Math.max(0, (String(step).split(".")[1] ?? "").length));
  return value.toFixed(decimals).replace(".", ",");
}

const STORE_KEY = "pwamote.action-values";

export interface KeyValueStore {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

/** Dernières valeurs saisies, par bouton, mémorisées sur le téléphone. */
export class RememberedValues {
  constructor(private store: KeyValueStore | null = typeof localStorage === "undefined" ? null : localStorage) {}

  private all(): Record<string, Values> {
    try {
      const raw = this.store?.getItem(STORE_KEY);
      const parsed = raw ? JSON.parse(raw) : {};
      return parsed && typeof parsed === "object" ? parsed : {};
    } catch {
      return {};
    }
  }

  get(actionId: string): Values | undefined {
    return this.all()[actionId];
  }

  set(actionId: string, values: Values): void {
    try {
      this.store?.setItem(STORE_KEY, JSON.stringify({ ...this.all(), [actionId]: values }));
    } catch {
      /* stockage plein ou interdit : tant pis, on reprendra les valeurs par défaut */
    }
  }
}
