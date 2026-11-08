import { For, Show, type JSX } from "solid-js";
import type { MagnifierFollow } from "../generated/MagnifierFollow";
import type { PointerMode } from "../generated/PointerMode";
import type { Settings } from "../generated/Settings";
import type { RemoteController } from "../lib/remote";

function Segmented<T extends string>(props: { value: T; options: [T, string][]; onChange: (v: T) => void }) {
  return (
    <div class="seg" role="radiogroup">
      <For each={props.options}>
        {([v, label]) => (
          <button role="radio" aria-checked={props.value === v} onClick={() => props.onChange(v)}>
            {label}
          </button>
        )}
      </For>
    </div>
  );
}

function Slider(props: { label: string; value: number; min: number; max: number; step: number; format?: (v: number) => string; onChange: (v: number) => void }) {
  return (
    <div class="field col">
      <div class="head">
        <span class="lbl">{props.label}</span>
        <span class="val">{(props.format ?? ((v) => String(v)))(props.value)}</span>
      </div>
      <input type="range" min={props.min} max={props.max} step={props.step} value={props.value} onInput={(e) => props.onChange(parseFloat(e.currentTarget.value))} />
    </div>
  );
}

function Toggle(props: { label: string; checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <div class="field">
      <span class="lbl">{props.label}</span>
      <span class="switch">
        <input type="checkbox" checked={props.checked} onChange={(e) => props.onChange(e.currentTarget.checked)} />
        <i />
      </span>
    </div>
  );
}

export function SettingsSheet(props: { remote: RemoteController; onClose: () => void }): JSX.Element {
  const r = props.remote;
  const s = () => r.settings();
  const set = (f: (s: Settings) => Settings, key: string) => r.edit(f, key);
  const pointer = (p: Partial<Settings["pointer"]>) =>
    set((x) => ({ ...x, pointer: { ...x.pointer, ...p } }), `pointer.${Object.keys(p).join(",")}`);
  const magnifier = (p: Partial<Settings["magnifier"]>) =>
    set((x) => ({ ...x, magnifier: { ...x.magnifier, ...p } }), `magnifier.${Object.keys(p).join(",")}`);

  return (
    <>
      <div class="sheet-back" onClick={props.onClose} />
      <div class="glass sheet" role="dialog" aria-label="Réglages">
        <div class="grab" />
        <h2>Réglages</h2>
        <Show when={s()} fallback={<p class="note">Connexion au PC…</p>}>
          {(cfg) => (
            <>
              <h3>Pointage</h3>
              <div class="field col">
                <span class="lbl">Mode</span>
                <Segmented<PointerMode>
                  value={cfg().pointer.mode}
                  options={[
                    ["air_hold", "Gyro (pouce)"],
                    ["air_always", "Gyro continu"],
                    ["trackpad", "Trackpad"],
                  ]}
                  onChange={(mode) => pointer({ mode })}
                />
              </div>
              <Slider label="Sensibilité" min={5} max={120} step={1} value={cfg().pointer.sensitivity} format={(v) => `${Math.round(v)}`} onChange={(sensitivity) => pointer({ sensitivity })} />
              <Slider label="Accélération" min={0} max={1} step={0.05} value={cfg().pointer.acceleration} format={(v) => `${Math.round(v * 100)} %`} onChange={(acceleration) => pointer({ acceleration })} />
              <Slider label="Lissage" min={0} max={1} step={0.05} value={cfg().pointer.smoothing} format={(v) => `${Math.round(v * 100)} %`} onChange={(smoothing) => pointer({ smoothing })} />
              <Slider label="Zone morte" min={0} max={6} step={0.1} value={cfg().pointer.deadzone_dps} format={(v) => `${v.toFixed(1)} °/s`} onChange={(deadzone_dps) => pointer({ deadzone_dps })} />
              <Slider label="Vitesse du trackpad" min={0.5} max={12} step={0.1} value={cfg().pointer.trackpad_speed} format={(v) => `×${v.toFixed(1)}`} onChange={(trackpad_speed) => pointer({ trackpad_speed })} />
              <Toggle label="Inverser gauche / droite" checked={cfg().pointer.invert_x} onChange={(invert_x) => pointer({ invert_x })} />
              <Toggle label="Inverser l'axe vertical" checked={cfg().pointer.invert_y} onChange={(invert_y) => pointer({ invert_y })} />
              <Toggle label="Défilement naturel" checked={cfg().pointer.natural_scroll} onChange={(natural_scroll) => pointer({ natural_scroll })} />

              <h3>Loupe</h3>
              <Slider label="Grossissement" min={1.25} max={6} step={0.25} value={cfg().magnifier.zoom} format={(v) => `×${v.toFixed(2).replace(/0$/, "")}`} onChange={(zoom) => magnifier({ zoom })} />
              <div class="field col">
                <span class="lbl">Suivre</span>
                <Segmented<MagnifierFollow>
                  value={cfg().magnifier.follow}
                  options={[
                    ["smart", "Intelligent"],
                    ["cursor", "Curseur"],
                    ["caret", "Saisie"],
                  ]}
                  onChange={(follow) => magnifier({ follow })}
                />
                <p class="note">Intelligent : la loupe suit la zone de texte pendant la frappe, puis revient au curseur.</p>
              </div>

              <h3>Clavier et retour</h3>
              <Toggle label="Modificateurs collants" checked={cfg().keyboard.sticky_modifiers} onChange={(sticky_modifiers) => set((x) => ({ ...x, keyboard: { sticky_modifiers } }), "keyboard.sticky")} />
              <Toggle label="Retour haptique" checked={cfg().feedback.haptics} onChange={(haptics) => set((x) => ({ ...x, feedback: { haptics } }), "feedback.haptics")} />

              <h3>Appareil</h3>
              <div class="row">
                <button class="btn ghost small" onClick={() => r.recenter()}>
                  Recentrer le curseur
                </button>
                <button class="btn ghost small" onClick={() => r.resetGyroAxes()}>
                  Recalibrer le gyroscope
                </button>
                <button class="btn ghost small" onClick={() => r.forget()}>
                  Oublier ce PC
                </button>
              </div>
              <p class="note">Les réglages sont enregistrés sur le PC : tous vos téléphones partagent les mêmes.</p>
            </>
          )}
        </Show>
      </div>
    </>
  );
}
