import { For, onMount } from "solid-js";
import type { Key } from "../generated/Key";
import type { Modifier } from "../generated/Modifier";
import { clipFromDataTransfer } from "../lib/clipboard";
import { TextSync, isAndroid } from "../lib/text-sync";
import type { RemoteController } from "../lib/remote";

const MODS: [Modifier, string][] = [
  ["ctrl", "Ctrl"],
  ["shift", "Maj"],
  ["alt", "Alt"],
  ["meta", "Win"],
];
const KEYS: [Key, string][] = [
  ["escape", "Échap"],
  ["tab", "Tab"],
  ["delete", "Suppr"],
  ["arrow_left", "←"],
  ["arrow_up", "↑"],
  ["arrow_down", "↓"],
  ["arrow_right", "→"],
  ["home", "Début"],
  ["end", "Fin"],
  ["page_up", "Pg ↑"],
  ["page_down", "Pg ↓"],
  ["f5", "F5"],
];

export function Keyboard(props: { remote: RemoteController }) {
  const r = props.remote;
  let input!: HTMLInputElement;
  const text = new TextSync({ backspace: (n) => r.backspace(n), text: (t) => r.text(t) }, { waitForComposition: !isAndroid() });

  onMount(() => input.focus());

  const resetField = () => {
    input.value = "";
    text.reset();
  };
  const after = (tooLong: boolean) => tooLong && resetField();
  const enter = () => {
    // Le mot en cours de composition (Android) part avant la touche Entrée.
    text.flush(input.value);
    r.tap("enter");
    resetField();
  };

  /**
   * Touche d'action. Elle ne prend pas le focus : le clavier du téléphone reste ouvert s'il
   * l'était, et reste fermé s'il a été replié pour mieux voir l'écran.
   */
  const act = (fn: () => void) => ({
    onPointerDown: (e: PointerEvent) => e.preventDefault(),
    onMouseDown: (e: MouseEvent) => e.preventDefault(),
    onClick: fn,
  });

  return (
    <div class="glass kb">
      <input
        ref={input}
        class="kb-input"
        type="text"
        placeholder="Écrivez ici, ça arrive sur le PC"
        autocapitalize="sentences"
        autocomplete="off"
        spellcheck={true}
        enterkeyhint="send"
        onCompositionStart={() => text.compositionStart()}
        onCompositionEnd={() => after(text.compositionEnd(input.value))}
        onInput={() => after(text.input(input.value))}
        onPaste={(e) => {
          // Un texte collé est tapé comme d'habitude ; une image est collée sur le PC (Ctrl+V).
          const clip = e.clipboardData && clipFromDataTransfer(e.clipboardData);
          if (!clip) return;
          e.preventDefault();
          void r.pasteOnPc(clip);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            enter();
          } else if (e.key === "Backspace" && input.value === "") {
            // Champ vide : l'effacement vise le texte déjà présent sur le PC.
            r.backspace(1);
          }
        }}
      />
      <div class="keys">
        <For each={MODS}>
          {([m, label]) => (
            <button class="key" data-mod={r.activeModifiers().includes(m) ? r.modifiers.get(m) : "off"} tabindex="-1" {...act(() => r.pressModifier(m))}>
              {label}
            </button>
          )}
        </For>
        <button
          class="key"
          tabindex="-1"
          {...act(() => {
            // On efface dans le champ aussi, pour que les deux restent d'accord.
            if (input.value !== "") {
              input.value = Array.from(input.value).slice(0, -1).join("");
              after(text.flush(input.value));
            } else {
              r.backspace(1);
            }
          })}
        >
          ⌫
        </button>
        <button
          class="key"
          tabindex="-1"
          {...act(enter)}
        >
          ⏎
        </button>
        <For each={KEYS}>
          {([k, label]) => (
            <button class="key" tabindex="-1" {...act(() => r.tap(k))}>
              {label}
            </button>
          )}
        </For>
      </div>
    </div>
  );
}
