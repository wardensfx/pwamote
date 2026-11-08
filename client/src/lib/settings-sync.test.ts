import { describe, expect, it } from "vitest";
import type { Settings } from "../generated/Settings";
import { SettingsSync } from "./settings-sync";

const base = (): Settings => ({
  pointer: { mode: "air_hold", sensitivity: 38, acceleration: 0.45, smoothing: 0.35, deadzone_dps: 1.6, invert_y: false, trackpad_speed: 1.6, scroll_speed: 1, natural_scroll: true },
  magnifier: { zoom: 2.5, follow: "smart", animation_ms: 220, caret_hold_ms: 1500 },
  keyboard: { sticky_modifiers: true },
  feedback: { haptics: true },
});

const invert = (s: Settings): Settings => ({ ...s, pointer: { ...s.pointer, invert_y: true } });
const zoom = (z: number) => (s: Settings): Settings => ({ ...s, magnifier: { ...s.magnifier, zoom: z } });

function setup() {
  const seen: Settings[] = [];
  const sync = new SettingsSync((s) => seen.push(s));
  sync.onServer({ rev: 0, settings: base() });
  return { sync, seen };
}

describe("SettingsSync", () => {
  it("n'affiche rien avant le premier état du serveur", () => {
    const sync = new SettingsSync(() => {});
    expect(sync.current).toBeNull();
    sync.edit(invert);
    expect(sync.flush()).toBeNull();
  });

  it("applique la modification tout de suite et l'envoie avec la révision connue", () => {
    const { sync, seen } = setup();
    sync.edit(invert);
    expect(seen.at(-1)!.pointer.invert_y).toBe(true);
    const msg = sync.flush()!;
    expect(msg.base_rev).toBe(0);
    expect(msg.settings.pointer.invert_y).toBe(true);
  });

  it("n'envoie pas de seconde mise à jour tant que la première n'est pas confirmée", () => {
    const { sync } = setup();
    sync.edit(invert);
    sync.flush();
    sync.edit(zoom(4));
    expect(sync.flush()).toBeNull();
  });

  it("la confirmation du serveur solde les modifications et déclenche la suite", () => {
    const { sync } = setup();
    sync.edit(invert);
    sync.flush();
    sync.edit(zoom(4));
    const next = sync.onServer({ rev: 1, settings: invert(base()) })!;
    expect(next.base_rev).toBe(1);
    expect(next.settings.magnifier.zoom).toBe(4);
    expect(next.settings.pointer.invert_y).toBe(true);
    expect(sync.onServer({ rev: 2, settings: next.settings })).toBeNull();
    expect(sync.pending).toBe(false);
  });

  it("conserve les modifications locales quand un autre téléphone change autre chose", () => {
    const { sync } = setup();
    sync.edit(invert);
    sync.flush();
    // Un autre téléphone a gagné la course avec un réglage différent.
    const retry = sync.onServer({ rev: 1, settings: zoom(3)(base()) })!;
    expect(retry.base_rev).toBe(1);
    expect(retry.settings.pointer.invert_y).toBe(true);
    expect(retry.settings.magnifier.zoom).toBe(3);
  });

  it("ignore un état plus ancien que celui déjà connu", () => {
    const { sync } = setup();
    sync.onServer({ rev: 5, settings: zoom(5)(base()) });
    sync.onServer({ rev: 3, settings: zoom(3)(base()) });
    expect(sync.rev).toBe(5);
    expect(sync.current!.magnifier.zoom).toBe(5);
  });

  it("peut renvoyer après une déconnexion", () => {
    const { sync } = setup();
    sync.edit(invert);
    sync.flush();
    sync.onDisconnect();
    expect(sync.flush()).not.toBeNull();
  });
});
