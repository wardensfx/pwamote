import { describe, expect, it } from "vitest";
import { StickyModifiers } from "./modifiers";

describe("StickyModifiers", () => {
  it("arme, verrouille puis relâche", () => {
    const m = new StickyModifiers(true);
    expect(m.press("ctrl")).toBe("once");
    expect(m.press("ctrl")).toBe("locked");
    expect(m.press("ctrl")).toBe("off");
  });

  it("consume ne libère que les modificateurs armés une fois", () => {
    const m = new StickyModifiers(true);
    m.press("ctrl");
    m.press("shift");
    m.press("shift");
    expect(m.active().sort()).toEqual(["ctrl", "shift"]);
    expect(m.consume()).toEqual(["ctrl"]);
    expect(m.active()).toEqual(["shift"]);
  });

  it("désactiver le mode collant remet tout à zéro", () => {
    const m = new StickyModifiers(true);
    m.press("alt");
    m.setSticky(false);
    expect(m.active()).toEqual([]);
  });
});
