import { Match, Show, Switch, createSignal } from "solid-js";
import type { RemoteController } from "../lib/remote";
import { Keyboard } from "./Keyboard";
import { Pad } from "./Pad";
import { SettingsSheet } from "./Settings";
import { CenterIcon, GearIcon, KeyboardIcon, LoupeIcon } from "./icons";

const Mark = () => (
  <div class="mark">
    <svg viewBox="0 0 512 512" aria-hidden="true">
      <rect x="176" y="84" width="160" height="344" rx="44" fill="none" stroke="currentColor" stroke-opacity=".5" stroke-width="22" />
      <circle cx="256" cy="196" r="38" fill="var(--accent)" />
    </svg>
    PwaMote
  </div>
);

function Pairing(props: { remote: RemoteController }) {
  return (
    <div class="center">
      <Mark />
      <h1>Associez votre PC</h1>
      <p>Lancez PwaMote sur l'ordinateur, puis scannez le QR code affiché dans sa fenêtre avec l'appareil photo du téléphone.</p>
    </div>
  );
}

function Problem(props: { remote: RemoteController }) {
  return (
    <div class="center">
      <Mark />
      <h1>Connexion impossible</h1>
      <p>{props.remote.error() ?? "Le PC ne répond pas."}</p>
      <div class="row">
        <button class="btn" onClick={() => props.remote.start()}>
          Réessayer
        </button>
        <Show when={props.remote.paired}>
          <button class="btn ghost" onClick={() => props.remote.forget()}>
            Oublier ce PC
          </button>
        </Show>
      </div>
    </div>
  );
}

function Remote(props: { remote: RemoteController }) {
  const r = props.remote;
  const [keyboard, setKeyboard] = createSignal(false);
  const [settings, setSettings] = createSignal(false);
  const down = (b: "left" | "right") => {
    const [on, set] = createSignal(false);
    return {
      on,
      handlers: {
        onPointerDown: (e: PointerEvent) => {
          (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
          set(true);
          r.button(b, true);
        },
        onPointerUp: () => {
          if (on()) r.button(b, false);
          set(false);
        },
        onPointerCancel: () => {
          if (on()) r.button(b, false);
          set(false);
        },
        onContextMenu: (e: Event) => e.preventDefault(),
      },
    };
  };
  const left = down("left");
  const right = down("right");

  return (
    <>
      <div class="topbar">
        <div class="status">
          <span class="dot" data-s={r.phase()} />
          <span>
            <b>{r.serverName() || "PC"}</b>
            {r.phase() === "ready" ? "" : " · reconnexion…"}
          </span>
        </div>
        <button class="icon-btn" aria-label="Réglages" onClick={() => setSettings(true)}>
          <GearIcon />
        </button>
      </div>

      <Pad remote={r} onTouch={() => setKeyboard(false)} />

      <Show when={keyboard()}>
        <Keyboard remote={r} />
      </Show>

      <div class="clicks">
        <button class="glass click" data-side="left" data-down={left.on()} {...left.handlers}>
          Clic gauche
        </button>
        <button class="glass click" data-side="right" data-down={right.on()} {...right.handlers}>
          Clic droit
        </button>
      </div>

      <div class="chips">
        <button class="glass chip" aria-pressed={r.magnifier().active} onClick={() => r.magnify("toggle")}>
          <LoupeIcon /> Loupe
        </button>
        <button class="glass chip" onClick={() => r.recenter()}>
          <CenterIcon /> Centrer
        </button>
        <button class="glass chip" aria-pressed={keyboard()} onClick={() => setKeyboard(!keyboard())}>
          <KeyboardIcon /> Clavier
        </button>
      </div>

      <Show when={settings()}>
        <SettingsSheet remote={r} onClose={() => setSettings(false)} />
      </Show>
    </>
  );
}

export function App(props: { remote: RemoteController }) {
  const r = props.remote;
  return (
    <>
      <div class="stage" />
      <main class="app">
        <Switch fallback={<Remote remote={r} />}>
          <Match when={r.phase() === "needs-pairing"}>
            <Pairing remote={r} />
          </Match>
          <Match when={r.phase() === "rejected"}>
            <Problem remote={r} />
          </Match>
        </Switch>
      </main>
    </>
  );
}
