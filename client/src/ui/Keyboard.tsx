import { For, onMount } from "solid-js";
import type { Key } from "../generated/Key";
import type { Modifier } from "../generated/Modifier";
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
  let composing = false;

  onMount(() => input.focus());

  const flush = () => {
    if (composing || input.value === "") return;
    r.text(input.value);
    input.value = "";
  };

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
          flush();
        }}
        onInput={flush}
        onKeyDown={(e) => {
          // Les touches sans texte ne déclenchent pas `input` : on les relaie ici.
          if (e.key === "Backspace" && input.value === "") {
            e.preventDefault();
            r.tap("backspace");
          } else if (e.key === "Enter") {
            e.preventDefault();
            flush();
            r.tap("enter");
          }
        }}
      />
      <div class="keys">
        <For each={MODS}>
          {([m, label]) => (
            <button
              class="key"
              data-mod={r.activeModifiers().includes(m) ? r.modifiers.get(m) : "off"}
              onPointerDown={(e) => {
                e.preventDefault(); // garde le focus (et le clavier) sur le champ
                r.pressModifier(m);
              }}
            >
              {label}
            </button>
          )}
        </For>
        <button class="key" onPointerDown={(e) => (e.preventDefault(), r.tap("backspace"))}>
          ⌫
        </button>
        <button class="key" onPointerDown={(e) => (e.preventDefault(), r.tap("enter"))}>
          ⏎
        </button>
        <For each={KEYS}>
          {([k, label]) => (
            <button class="key" onPointerDown={(e) => (e.preventDefault(), r.tap(k))}>
              {label}
            </button>
          )}
        </For>
      </div>
    </div>
  );
}
