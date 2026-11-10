import { For, Show, createEffect, createMemo, createSignal, on, onCleanup, onMount } from "solid-js";
import { DeckEditor } from "./DeckEditor";
import { PublicCertCard } from "./PublicCert";
import { countdown, isValidHost, phonesLabel, relativeTime, selectedAddress } from "./format";
import type { AppConfig, Backend, DeviceView, PairingInfo, ServerStatus } from "./ipc";

/** Durée de validité d'un code d'appairage, en secondes (voir pwamote-core). */
const CODE_TTL = 300;

function Toggle(props: { checked: boolean; onChange: (v: boolean) => void; label: string; big?: boolean }) {
  return (
    <label class="switch" classList={{ big: props.big }} aria-label={props.label}>
      <input type="checkbox" role="switch" checked={props.checked} onChange={(e) => props.onChange(e.currentTarget.checked)} />
      <i />
    </label>
  );
}

function Row(props: { title: string; hint?: string; children: any }) {
  return (
    <div class="row">
      <div class="row-text">
        <div class="row-title">{props.title}</div>
        <Show when={props.hint}>
          <div class="row-hint">{props.hint}</div>
        </Show>
      </div>
      {props.children}
    </div>
  );
}

export function App(props: { backend: Backend }) {
  const be = props.backend;
  const [status, setStatus] = createSignal<ServerStatus | null>(null);
  const [devices, setDevices] = createSignal<DeviceView[]>([]);
  const [pairing, setPairing] = createSignal<PairingInfo | null>(null);
  const [pairedAt, setPairedAt] = createSignal(0);
  const [now, setNow] = createSignal(Math.floor(Date.now() / 1000));
  const [backdrop, setBackdrop] = createSignal(false);
  const [dataDir, setDataDir] = createSignal("");
  const [copied, setCopied] = createSignal(false);
  const [customHost, setCustomHost] = createSignal("");
  const [customMode, setCustomMode] = createSignal(false);
  const [portDraft, setPortDraft] = createSignal<string | null>(null);
  const [tab, setTab] = createSignal<"connection" | "actions">("connection");
  const [deckDirty, setDeckDirty] = createSignal(false);
  // Ne remplace l'état que s'il a vraiment changé : sinon la liste d'adresses et le QR se redessinent
  // à chaque interrogation périodique.
  const updateStatus = (next: ServerStatus) => {
    if (JSON.stringify(next) !== JSON.stringify(status())) setStatus(next);
  };
  const updateDevices = (next: DeviceView[]) => {
    if (JSON.stringify(next) !== JSON.stringify(devices())) setDevices(next);
  };

  const refreshPairing = async () => {
    try {
      setPairing(await be.newPairing());
      setPairedAt(Math.floor(Date.now() / 1000));
    } catch {
      setPairing(null);
    }
  };

  const apply = async (patch: Partial<AppConfig>) => {
    const s = status();
    if (s) updateStatus(await be.setConfig({ ...s.config, ...patch }));
  };

  onMount(async () => {
    setBackdrop(await be.hasBackdrop());
    setDataDir(await be.dataDir());
    const first = await be.getStatus();
    setStatus(first);
    if (first.config.address.kind === "custom") {
      setCustomHost(first.config.address.host);
      setCustomMode(true);
    }
    setDevices(await be.listDevices());
    const unlisten = await be.onStatusChanged(updateStatus);
    const poll = setInterval(async () => {
      updateStatus(await be.getStatus());
      updateDevices(await be.listDevices());
    }, 2000);
    const tick = setInterval(() => setNow(Math.floor(Date.now() / 1000)), 1000);
    onCleanup(() => (unlisten(), clearInterval(poll), clearInterval(tick)));
  });

  // Nouveau QR quand : l'adresse change, le serveur (re)démarre, un téléphone vient de s'appairer, ou le code expire.
  // Les mémos évitent de réagir à chaque interrogation périodique : seul un vrai changement compte.
  const origin = createMemo(() => status()?.origin ?? null);
  const deviceCount = createMemo(() => devices().length);
  createEffect(on(origin, (o) => (o ? void refreshPairing() : setPairing(null))));
  createEffect(on(deviceCount, (n, prev) => prev !== undefined && n > prev && void refreshPairing()));
  const expiresIn = () => CODE_TTL - (now() - pairedAt());
  createEffect(() => {
    if (pairing() && expiresIn() <= 0) void refreshPairing();
  });

  const copy = async () => {
    const p = pairing();
    if (!p) return;
    try {
      await navigator.clipboard.writeText(p.url);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      /* presse-papiers indisponible */
    }
  };

  const onAddressChange = (value: string) => {
    const s = status();
    if (!s) return;
    if (value === "custom") {
      // La saisie s'affiche ; l'adresse ne change qu'à la validation.
      setCustomMode(true);
      return;
    }
    setCustomMode(false);
    const c = s.candidates.find((c) => (c.choice.kind === "ip" ? `ip:${c.choice.ip}` : c.choice.kind) === value);
    if (c) void apply({ address: c.choice });
  };

  const customValid = () => isValidHost(customHost());
  const portValue = () => portDraft() ?? String(status()?.config.port ?? "");
  const commitPort = () => {
    const n = Number(portDraft());
    setPortDraft(null);
    if (Number.isInteger(n) && n >= 1024 && n <= 65535) void apply({ port: n });
  };

  return (
    <main classList={{ backdrop: backdrop() }}>
      <Show when={status()} fallback={<div class="loading">Démarrage…</div>}>
        {(s) => (
          <>
            <header>
              <div class="brand">
                <svg viewBox="0 0 512 512" aria-hidden="true">
                  <rect x="176" y="84" width="160" height="344" rx="44" fill="none" stroke="currentColor" stroke-opacity=".55" stroke-width="22" />
                  <circle cx="256" cy="196" r="38" fill="var(--accent)" />
                </svg>
                <div>
                  <h1>PwaMote</h1>
                  <div class="machine">{s().machine}</div>
                </div>
              </div>
              <div class="power">
                <span class="power-label">{s().config.server_enabled ? "Serveur actif" : "Serveur arrêté"}</span>
                <Toggle big checked={s().config.server_enabled} label="Activer le serveur" onChange={async (v) => setStatus(await be.setEnabled(v))} />
              </div>
            </header>

            <nav class="tabs" role="tablist">
              <button role="tab" aria-selected={tab() === "connection"} onClick={() => setTab("connection")}>
                Connexion
              </button>
              <button role="tab" aria-selected={tab() === "actions"} onClick={() => setTab("actions")}>
                Actions
                <Show when={deckDirty()}>
                  <i class="tab-dot" aria-label="modifications non enregistrées" />
                </Show>
              </button>
            </nav>

            {/* Les deux onglets restent montés : un brouillon d'actions survit à un détour par la connexion. */}
            <div class="tab-panel" hidden={tab() !== "actions"}>
              <DeckEditor backend={be} onDirty={setDeckDirty} />
            </div>

            <div class="tab-panel" hidden={tab() !== "connection"}>
            <section class="glass status" data-state={s().error ? "error" : s().running ? "ok" : "off"}>
              <span class="dot" />
              <div class="status-text">
                <Show when={s().running} fallback={<b>{s().error ? "Le serveur n'a pas pu démarrer" : "Les téléphones ne peuvent pas se connecter"}</b>}>
                  <b>En ligne</b> · <span class="mono">{s().host}:{s().port}</span>
                </Show>
                <div class="sub">{s().error ?? (s().running ? phonesLabel(s().connected) : "Activez le serveur pour associer un téléphone.")}</div>
              </div>
            </section>

            <section class="glass pair">
              <h2>Associer un téléphone</h2>
              <div class="qr-wrap">
                <Show
                  when={!(s().running && s().public_cert.phase === "obtaining")}
                  fallback={
                    <div class="qr off setup" role="status" aria-live="polite">
                      <span class="spinner" aria-hidden="true" />
                      <b>Paramétrage du HTTPS…</b>
                      <span class="setup-step">{s().public_cert.message ?? "Préparation du certificat"}</span>
                      <span class="setup-hint">Le QR code apparaîtra dès que le certificat sera prêt.</span>
                    </div>
                  }
                >
                <Show
                  when={pairing()}
                  fallback={
                    <div class="qr off">
                      <span>{s().running ? "Création du code…" : "Serveur arrêté"}</span>
                      <Show when={!s().running && !s().error}>
                        <button class="btn small" onClick={async () => setStatus(await be.setEnabled(true))}>
                          Démarrer
                        </button>
                      </Show>
                    </div>
                  }
                >
                  {(p) => <div class="qr" innerHTML={p().svg} />}
                </Show>
                </Show>
              </div>
              <Show when={pairing() && s().public_cert.phase !== "obtaining"}>
                <div class="expiry">Code valable encore {countdown(expiresIn())} · à usage unique</div>
              </Show>

              <div class="actions">
                <button class="btn ghost small" onClick={copy} disabled={!pairing()}>
                  {copied() ? "Copié" : "Copier le lien"}
                </button>
                <button class="btn ghost small" onClick={refreshPairing} disabled={!s().running}>
                  Nouveau code
                </button>
              </div>

              <details hidden={s().public_cert.phase === "active"}>
                <summary>Première fois sur un iPhone ?</summary>
                <ol>
                  <li>
                    Sur le téléphone, ouvrez <span class="mono">{s().origin ?? "…"}/pwamote.mobileconfig</span> et autorisez le téléchargement.
                  </li>
                  <li>Réglages › Profil téléchargé › Installer.</li>
                  <li>Réglages › Général › Informations › Réglages des certificats : activez la confiance pour « PwaMote Local CA ».</li>
                  <li>Scannez ensuite le QR code ci-dessus.</li>
                </ol>
              </details>

            </section>

            <section class="glass devices">
              <h2>Appareils associés</h2>
              <Show when={devices().length > 0} fallback={<p class="empty">Aucun appareil pour l'instant.</p>}>
                <For each={devices()}>
                  {(d) => (
                    <div class="device">
                      <div>
                        <div class="row-title">{d.name}</div>
                        <div class="row-hint">Vu {relativeTime(now(), d.last_seen)}</div>
                      </div>
                      <button class="btn danger small" onClick={async () => (await be.revokeDevice(d.id), setDevices(await be.listDevices()))}>
                        Retirer
                      </button>
                    </div>
                  )}
                </For>
              </Show>
            </section>

            <details class="advanced">
              <summary>Paramètres avancés</summary>
              <div class="advanced-body">
            <PublicCertCard
              config={s().config.public_cert}
              status={s().public_cert}
              now={now()}
              onSave={async (public_cert) => apply({ public_cert })}
            />

            <section class="glass options">
              <h2>Options</h2>
              <Row title="Lancer avec Windows" hint="PwaMote démarre à l'ouverture de votre session.">
                <Toggle checked={s().config.launch_at_login} label="Lancer avec Windows" onChange={(v) => apply({ launch_at_login: v })} />
              </Row>
              <Row title="Démarrer réduit" hint="Au lancement, reste dans la barre système sans ouvrir cette fenêtre.">
                <Toggle checked={s().config.start_minimized} label="Démarrer réduit" onChange={(v) => apply({ start_minimized: v })} />
              </Row>
              <Row title="Réduire dans la barre système à la fermeture" hint="Sinon, fermer la fenêtre quitte PwaMote.">
                <Toggle checked={s().config.close_to_tray} label="Réduire à la fermeture" onChange={(v) => apply({ close_to_tray: v })} />
              </Row>
              <Row title="Port" hint="Entre 1024 et 65535. Le serveur redémarre.">
                <input
                  class="port"
                  inputmode="numeric"
                  value={portValue()}
                  onInput={(e) => setPortDraft(e.currentTarget.value)}
                  onBlur={commitPort}
                  onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
                  aria-label="Port"
                />
              </Row>
            </section>

            <section class="glass options">
              <h2>Adresse</h2>
              <label class="field-label" for="addr">Adresse locale (QR code sans certificat public)</label>
              <select
                id="addr"
                value={customMode() ? "custom" : selectedAddress(s().config, s().candidates)}
                onChange={(e) => onAddressChange(e.currentTarget.value)}
              >
                <For each={s().candidates}>
                  {(c) => (
                    <option value={c.choice.kind === "ip" ? `ip:${c.choice.ip}` : c.choice.kind}>
                      {c.label}
                      {c.recommended ? " (recommandée)" : c.choice.kind === "mdns" ? " (nom, dépend du mDNS)" : ""}
                    </option>
                  )}
                </For>
                <option value="custom">Autre adresse…</option>
              </select>
              <Show when={customMode()}>
                <div class="custom">
                  <input
                    type="text"
                    placeholder="pc.maison ou 172.16.0.5"
                    value={customHost()}
                    aria-invalid={customHost() !== "" && !customValid()}
                    onInput={(e) => setCustomHost(e.currentTarget.value)}
                    onKeyDown={(e) => e.key === "Enter" && customValid() && void apply({ address: { kind: "custom", host: customHost().trim().toLowerCase() } })}
                  />
                  <button class="btn small" disabled={!customValid()} onClick={() => apply({ address: { kind: "custom", host: customHost().trim().toLowerCase() } })}>
                    Appliquer
                  </button>
                </div>
              </Show>
              <p class="hint">
                Si le téléphone ne trouve pas le PC, choisissez l'adresse IP : elle fonctionne sur tous les réseaux. Le nom .local est stable
                si l'IP change, mais dépend du mDNS.
              </p>

            </section>

              </div>
            </details>

            </div>

            <footer>
              <span>Données : <span class="mono">{dataDir()}</span></span>
              <span>Clic droit sur l'icône de la barre système pour quitter.</span>
            </footer>
          </>
        )}
      </Show>
    </main>
  );
}
