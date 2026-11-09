import { For, onMount } from "solid-js";
import type { Key } from "../generated/Key";
import type { Modifier } from "../generated/Modifier";
import { clipFromDataTransfer } from "../lib/clipboard";
import { diffText } from "../lib/text-diff";
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

/** Au-delà, on repart d'un champ vide : le PC a déjà reçu tout le texte. */
const MAX_FIELD = 300;

export function Keyboard(props: { remote: RemoteController }) {
  const r = props.remote;
  let input!: HTMLInputElement;
  let composing = false;
  // Ce que le PC a déjà reçu du contenu actuel du champ.
  let sent = "";

  onMount(() => input.focus());

  const resetField = () => {
    input.value = "";
    sent = "";
  };

  const sync = () => {
    if (composing) return;
    const { backspaces, insert } = diffText(sent, input.value);
    sent = input.value;
    if (backspaces > 0) r.backspace(backspaces);
    if (insert) r.text(insert);
    if (sent.length > MAX_FIELD) resetField();
  };

  /** Touche d'action : garde le clavier du téléphone ouvert. */
  const act = (fn: () => void) => ({
    onPointerDown: (e: PointerEvent) => e.preventDefault(),
    onMouseDown: (e: MouseEvent) => e.preventDefault(),
    onClick: () => {
      fn();
      input.focus({ preventScroll: true });
    },
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
        onCompositionStart={() => (composing = true)}
        onCompositionEnd={() => {
          composing = false;
          sync();
        }}
        onInput={sync}
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
            sync();
            r.tap("enter");
            resetField();
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
              sync();
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
          {...act(() => {
            sync();
            r.tap("enter");
            resetField();
          })}
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
