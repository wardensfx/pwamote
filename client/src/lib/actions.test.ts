import { describe, expect, it } from "vitest";
import type { ActionInput } from "../generated/ActionInput";
import { defaultValue, finalValues, formatStep, initialValues, isValid, parseNumber, RememberedValues } from "./actions";

const choice: ActionInput = {
  id: "site",
  label: "Site",
  kind: { type: "choice", options: [{ label: "A", value: "a" }, { label: "B", value: "b" }], default: "zz" },
};
const slider: ActionInput = { id: "speed", label: "Vitesse", kind: { type: "slider", min: 0.5, max: 4, step: 0.25, default: 1, unit: "×" } };
const number: ActionInput = { id: "n", label: "N", kind: { type: "number", min: 1, max: null, default: 3, integer: true } };
const text: ActionInput = { id: "q", label: "Q", kind: { type: "text", default: "", placeholder: "" } };
const toggle: ActionInput = { id: "t", label: "T", kind: { type: "toggle", default: true, on: "1", off: "0" } };

describe("champs d'action", () => {
  it("prend la valeur par défaut déclarée, ou la première option", () => {
    expect(defaultValue(choice)).toBe("a");
    expect(defaultValue(slider)).toBe(1);
    expect(defaultValue(toggle)).toBe(true);
  });

  it("vérifie les valeurs selon le type", () => {
    expect(isValid(choice, "b")).toBe(true);
    expect(isValid(choice, "c")).toBe(false);
    expect(isValid(slider, 5)).toBe(false);
    expect(isValid(number, "12")).toBe(true);
    expect(isValid(number, "2,5")).toBe(false);
    expect(isValid(number, "0")).toBe(false);
    expect(isValid(text, "x")).toBe(true);
    expect(isValid(toggle, "true")).toBe(false);
  });

  it("lit les nombres avec une virgule", () => {
    expect(parseNumber("2,5")).toBe(2.5);
    expect(parseNumber(" -3 ")).toBe(-3);
    expect(parseNumber("1e9")).toBeNull();
    expect(parseNumber("")).toBeNull();
  });

  it("reprend les dernières valeurs encore valides", () => {
    const v = initialValues([choice, slider, number], { site: "b", speed: 99, n: 5 });
    expect(v).toEqual({ site: "b", speed: 1, n: 5 });
  });

  it("convertit les nombres saisis avant l'envoi", () => {
    expect(finalValues([number, text], { n: "7", q: "salut" })).toEqual({ n: 7, q: "salut" });
  });

  it("affiche les nombres avec la précision du pas", () => {
    expect(formatStep(1.25, 0.25)).toBe("1,25");
    expect(formatStep(3, 1)).toBe("3");
    expect(formatStep(0.30000000000000004, 0.1)).toBe("0,3");
  });

  it("mémorise les valeurs par bouton et survit à un stockage cassé", () => {
    const data = new Map<string, string>();
    const store = { getItem: (k: string) => data.get(k) ?? null, setItem: (k: string, v: string) => void data.set(k, v) };
    const r = new RememberedValues(store);
    r.set("a", { x: 1 });
    r.set("b", { y: "z" });
    expect(r.get("a")).toEqual({ x: 1 });
    expect(r.get("b")).toEqual({ y: "z" });
    data.set("pwamote.action-values", "{pas du json");
    expect(r.get("a")).toBeUndefined();
  });
});
