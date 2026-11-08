let label: HTMLLabelElement | null = null;

/**
 * Safari n'a pas `navigator.vibrate`, mais iOS 18 déclenche un retour haptique
 * quand on bascule un `<input type="checkbox" switch>`. On en garde un caché.
 */
function ensureSwitch(): HTMLLabelElement | null {
  if (typeof document === "undefined") return null;
  if (label?.isConnected) return label;
  label = document.createElement("label");
  label.setAttribute("aria-hidden", "true");
  label.style.cssText = "position:fixed;left:-100px;top:-100px;width:1px;height:1px;opacity:0;pointer-events:none";
  const input = document.createElement("input");
  input.type = "checkbox";
  input.setAttribute("switch", "");
  input.tabIndex = -1;
  label.appendChild(input);
  document.body.appendChild(label);
  return label;
}

export function haptic(kind: "tap" | "heavy" = "tap"): void {
  try {
    if (typeof navigator !== "undefined" && "vibrate" in navigator) {
      navigator.vibrate(kind === "heavy" ? 18 : 8);
      return;
    }
    ensureSwitch()?.click();
  } catch {
    /* pas de haptique disponible */
  }
}
