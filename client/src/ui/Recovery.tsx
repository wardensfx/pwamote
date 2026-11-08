import { Show, createSignal, onCleanup, onMount } from "solid-js";
import { parseScannedLink } from "../lib/identity";
import { probeServer, type Reachability } from "../lib/probe";
import type { RemoteController } from "../lib/remote";
import { forceUpdate } from "../lib/update";
import { QrScanner } from "./QrScanner";

/**
 * Lecture d'un QR code d'appairage, utilisable même déjà associé : même adresse, on s'associe de nouveau ;
 * autre adresse (IP du PC changée), on y va.
 */
export function useScanner(remote: RemoteController) {
  const [scanning, setScanning] = createSignal(false);
  const [bad, setBad] = createSignal(false);
  const onText = (text: string) => {
    setScanning(false);
    const link = parseScannedLink(text, location.origin);
    if (!link) return setBad(true);
    if (link.kind === "here") remote.start(link.code);
    else location.href = link.url;
  };
  const open = () => (setBad(false), setScanning(true));
  const view = () => (
    <>
      <Show when={bad()}>
        <p class="warn">Ce QR code n'est pas un code d'appairage PwaMote. Utilisez celui de la fenêtre du PC.</p>
      </Show>
      <Show when={scanning()}>
        <QrScanner onText={onText} onClose={() => setScanning(false)} />
      </Show>
    </>
  );
  return { open, view };
}

/** Boutons de secours, toujours accessibles, même sans aucune connexion au PC. */
export function RescueActions(props: { remote: RemoteController; onScan: () => void; retry?: boolean }) {
  return (
    <div class="row">
      <Show when={props.retry}>
        <button class="btn small" onClick={() => props.remote.start()}>
          Réessayer
        </button>
      </Show>
      <button class="btn ghost small" onClick={props.onScan}>
        Scanner le QR code
      </button>
      <button class="btn ghost small" onClick={() => void forceUpdate()}>
        Mettre à jour l'app
      </button>
      <Show when={props.remote.paired}>
        <button class="btn ghost small" onClick={() => props.remote.forget()}>
          Oublier ce PC
        </button>
      </Show>
    </div>
  );
}

/** Carte affichée à la place de la surface de pointage quand le PC ne répond plus. */
export function Recovery(props: { remote: RemoteController }) {
  const r = props.remote;
  const scanner = useScanner(r);
  const [reach, setReach] = createSignal<Reachability>("checking");

  const check = async () => {
    setReach("checking");
    setReach(await probeServer());
  };
  onMount(() => {
    void check();
    const t = setInterval(check, 5000);
    onCleanup(() => clearInterval(t));
  });

  return (
    <div class="glass pad recovery">
      <div class="recovery-body">
        <h2>Le PC ne répond pas</h2>
        <p class="mono-line">{location.host}</p>
        <p class="reach" data-reach={reach()}>
          {reach() === "checking"
            ? "Vérification…"
            : reach() === "reachable"
              ? "Le PC répond à cette adresse, mais la session ne s'ouvre pas. Réessayez, ou scannez un nouveau QR code."
              : "Rien ne répond à cette adresse. Nouvelle tentative automatique toutes les quelques secondes."}
        </p>
        <RescueActions remote={r} onScan={scanner.open} retry />
        <details>
          <summary>Que vérifier ?</summary>
          <ul>
            <li>Le téléphone est sur le même Wi-Fi que le PC.</li>
            <li>PwaMote est ouvert sur le PC et le serveur est actif (fenêtre ou icône de la barre système).</li>
            <li>L'adresse du PC a changé : scannez le QR code affiché dans sa fenêtre.</li>
            <li>
              Pare-feu Windows : après une recompilation ou une réinstallation, Windows peut bloquer le nouvel exécutable. Autorisez PwaMote sur les réseaux privés
              (Paramètres › Confidentialité et sécurité › Sécurité Windows › Pare-feu › Autoriser une application).
            </li>
          </ul>
        </details>
      </div>
      {scanner.view()}
    </div>
  );
}
