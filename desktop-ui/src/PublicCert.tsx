import { Show, createEffect, createSignal, on } from "solid-js";
import { publicCertSummary } from "./format";
import type { PublicCertConfig } from "../../client/src/generated/PublicCertConfig";
import type { PublicCertStatus } from "../../client/src/generated/PublicCertStatus";

type ProviderKind = "duck_dns" | "cloudflare";

const tokenOf = (c: PublicCertConfig) => (c.provider.kind === "duck_dns" ? c.provider.token : c.provider.api_token);

/**
 * Certificat Let's Encrypt : réglages (domaine, service DNS, jeton) et état de la tâche de fond.
 * Le formulaire est un brouillon : rien ne part au serveur avant « Enregistrer ».
 */
export function PublicCertCard(props: {
  config: PublicCertConfig;
  status: PublicCertStatus;
  now: number;
  onSave: (c: PublicCertConfig) => Promise<void>;
}) {
  const [kind, setKind] = createSignal<ProviderKind>(props.config.provider.kind);
  const [domain, setDomain] = createSignal(props.config.domain);
  const [token, setToken] = createSignal(tokenOf(props.config));
  const [email, setEmail] = createSignal(props.config.email);
  const [staging, setStaging] = createSignal(props.config.staging);
  const [saving, setSaving] = createSignal(false);

  // Une modification venue d'ailleurs (autre fenêtre, fichier) remplace le brouillon.
  createEffect(
    on(
      () => JSON.stringify(props.config),
      () => {
        setKind(props.config.provider.kind);
        setDomain(props.config.domain);
        setToken(tokenOf(props.config));
        setEmail(props.config.email);
        setStaging(props.config.staging);
      },
    ),
  );

  const draft = (enabled: boolean): PublicCertConfig => ({
    enabled,
    domain: domain().trim().toLowerCase(),
    email: email().trim(),
    staging: staging(),
    provider: kind() === "duck_dns" ? { kind: "duck_dns", token: token().trim() } : { kind: "cloudflare", api_token: token().trim() },
  });
  const dirty = () => JSON.stringify(draft(props.config.enabled)) !== JSON.stringify(props.config);
  const save = async (enabled: boolean) => {
    setSaving(true);
    try {
      await props.onSave(draft(enabled));
    } finally {
      setSaving(false);
    }
  };
  const summary = () => publicCertSummary(props.status, props.now);

  return (
    <section class="glass cert">
      <div class="row">
        <div class="row-text">
          <h2>Certificat Let's Encrypt</h2>
          <div class="row-hint">Plus de profil à installer sur l'iPhone : un vrai certificat, reconnu partout, pour un nom de domaine public.</div>
        </div>
        <label class="switch" aria-label="Utiliser Let's Encrypt">
          <input type="checkbox" role="switch" checked={props.config.enabled} disabled={saving()} onChange={(e) => void save(e.currentTarget.checked)} />
          <i />
        </label>
      </div>

      <div class="cert-state" data-tone={summary().tone}>
        <span class="dot" />
        <div>
          <b>{summary().title}</b>
          <Show when={summary().detail}>
            <div class="sub">{summary().detail}</div>
          </Show>
        </div>
      </div>

      <Show when={props.config.enabled}>
        <div class="segmented" role="radiogroup" aria-label="Service DNS">
          <button role="radio" aria-checked={kind() === "duck_dns"} onClick={() => setKind("duck_dns")}>
            DuckDNS <span>gratuit</span>
          </button>
          <button role="radio" aria-checked={kind() === "cloudflare"} onClick={() => setKind("cloudflare")}>
            Cloudflare <span>votre domaine</span>
          </button>
        </div>

        <label class="field-label" for="cert-domain">Nom de domaine du PC</label>
        <input
          id="cert-domain"
          type="text"
          spellcheck={false}
          placeholder={kind() === "duck_dns" ? "yann-pc.duckdns.org" : "pc.mondomaine.fr"}
          value={domain()}
          onInput={(e) => setDomain(e.currentTarget.value)}
        />

        <label class="field-label" for="cert-token">{kind() === "duck_dns" ? "Jeton DuckDNS" : "Jeton d'API Cloudflare (Zone · DNS · Modifier)"}</label>
        <input id="cert-token" type="password" spellcheck={false} autocomplete="off" value={token()} onInput={(e) => setToken(e.currentTarget.value)} />

        <label class="field-label" for="cert-email">E-mail de contact (facultatif)</label>
        <input id="cert-email" type="text" spellcheck={false} placeholder="Avertissements d'expiration de Let's Encrypt" value={email()} onInput={(e) => setEmail(e.currentTarget.value)} />

        <label class="check">
          <input type="checkbox" checked={staging()} onChange={(e) => setStaging(e.currentTarget.checked)} />
          Serveur de test Let's Encrypt (pour essayer sans risquer les limites ; certificat non reconnu)
        </label>

        <div class="actions">
          <button class="btn small" disabled={!dirty() || saving()} onClick={() => void save(true)}>
            {saving() ? "Enregistrement…" : "Enregistrer"}
          </button>
        </div>

        <details>
          <summary>Comment ça marche ?</summary>
          <ol>
            <li>
              {kind() === "duck_dns"
                ? "Créez un sous-domaine gratuit sur duckdns.org et copiez le jeton du compte."
                : "Créez un jeton d'API limité à la zone de votre domaine, droit DNS : Modifier."}
            </li>
            <li>PwaMote fait pointer ce nom vers l'adresse du PC sur votre réseau local et le tient à jour si elle change.</li>
            <li>Le certificat est obtenu par le défi DNS : le PC n'a jamais besoin d'être joignable depuis internet.</li>
            <li>Sans internet, le certificat local prend le relais ; PwaMote vérifie la connexion toutes les 5 minutes et renouvelle 30 jours avant l'échéance.</li>
            <li>Une fois le certificat obtenu, le QR code utilise ce nom. Les téléphones déjà associés par IP continuent de fonctionner.</li>
          </ol>
        </details>
      </Show>
    </section>
  );
}
