import { Match, Show, Switch, createEffect, createSignal, onCleanup } from "solid-js";
import { forceUpdate } from "../lib/update";
import type { RemoteController } from "../lib/remote";
import { ActionPanel } from "./Actions";
import { Keyboard } from "./Keyboard";
import { Pad } from "./Pad";
import { Recovery, RescueActions, useScanner } from "./Recovery";
import { SettingsSheet } from "./Settings";
import { CenterIcon, DeckIcon, GearIcon, KeyboardIcon, LoupeIcon } from "./icons";

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
  const scanner = useScanner(props.remote);
  return (
    <div class="center">
      <Mark />
      <h1>Associez votre PC</h1>
      <p>Lancez PwaMote sur l'ordinateur, puis scannez le QR code affiché dans sa fenêtre.</p>
      <div class="row">
        <button class="btn" onClick={scanner.open}>
          Scanner le QR code
        </button>
        <button class="btn ghost" onClick={() => void forceUpdate()}>
          Mettre à jour l'app
        </button>
      </div>
      {scanner.view()}
    </div>
  );
}

function Problem(props: { remote: RemoteController }) {
  const scanner = useScanner(props.remote);
  return (
    <div class="center">
      <Mark />
      <h1>Connexion impossible</h1>
      <p>{props.remote.error() ?? "Le PC ne répond pas."}</p>
      <RescueActions remote={props.remote} onScan={scanner.open} retry />
      {scanner.view()}
    </div>
  );
}

/** Replie le clavier du téléphone en retirant le focus du champ de saisie. */
const foldPhoneKeyboard = () => {
  const el = document.activeElement;
  if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) el.blur();
};

function Remote(props: { remote: RemoteController }) {
  const r = props.remote;
  // Un seul panneau à la fois entre la surface de pointage et les clics.
  const [panel, setPanel] = createSignal<"keyboard" | "actions" | null>(null);
  const toggle = (p: "keyboard" | "actions") => setPanel(panel() === p ? null : p);
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
  // Hors ligne depuis plus de 3 s : la surface de pointage laisse la place aux moyens de s'en sortir.
  const [stuck, setStuck] = createSignal(false);
  createEffect(() => {
    if (r.phase() === "ready") return setStuck(false);
    const t = setTimeout(() => setStuck(true), 3000);
    onCleanup(() => clearTimeout(t));
  });
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

      {/* Toucher la surface replie le clavier du téléphone pour mieux voir l'écran, mais garde le panneau
          et ses touches de contrôle : on le ferme avec le bouton Clavier, on rouvre la saisie en touchant le champ. */}
      <Show when={!stuck()} fallback={<Recovery remote={r} />}>
        <Pad remote={r} onTouch={foldPhoneKeyboard} />
      </Show>

      <Show when={panel() === "keyboard"}>
        <Keyboard remote={r} />
      </Show>
      <Show when={panel() === "actions"}>
        <ActionPanel remote={r} />
      </Show>
      <Show when={r.clipStatus()}>
        {(c) => (
          <div class="glass toast" classList={{ bad: c().state === "error", good: c().state === "ok" }} role="status">
            {c().message}
          </div>
        )}
      </Show>
      <Show when={r.actionError()}>
        <div class="glass toast bad" role="alert">
          {r.actionError()}
        </div>
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
        <button class="glass chip" aria-pressed={panel() === "keyboard"} onClick={() => toggle("keyboard")}>
          <KeyboardIcon /> Clavier
        </button>
        <button class="glass chip" aria-pressed={panel() === "actions"} onClick={() => toggle("actions")}>
          <DeckIcon /> Actions
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
