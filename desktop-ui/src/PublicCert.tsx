import { Show, createEffect, createSignal, on } from "solid-js";
import { publicCertSummary } from "./format";
import type { CertMode } from "../../client/src/generated/CertMode";
import type { PublicCertConfig } from "../../client/src/generated/PublicCertConfig";
import type { PublicCertStatus } from "../../client/src/generated/PublicCertStatus";

type ProviderKind = "duck_dns" | "cloudflare";

const tokenOf = (c: PublicCertConfig) => (c.provider.kind === "duck_dns" ? c.provider.token : c.provider.api_token);

const MODES: { mode: CertMode; label: string }[] = [
  { mode: "auto", label: "Automatique" },
  { mode: "custom", label: "Mon domaine" },
  { mode: "off", label: "Local seul" },
];

/**
 * HTTPS de l'iPhone. Par défaut automatique : le PC reçoit un nom `<id>.d.pwamote.app` et un
 * certificat Let's Encrypt, sans rien régler. Le domaine personnel reste possible ; « Local seul »
 * garde l'autorité locale (profil à installer sur l'iPhone).
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

  const draft = (mode: CertMode): PublicCertConfig => ({
    mode,
    domain: domain().trim().toLowerCase(),
    email: email().trim(),
    staging: staging(),
    provider: kind() === "duck_dns" ? { kind: "duck_dns", token: token().trim() } : { kind: "cloudflare", api_token: token().trim() },
  });
  const dirty = () => JSON.stringify(draft(props.config.mode)) !== JSON.stringify(props.config);
  const save = async (mode: CertMode) => {
    setSaving(true);
    try {
      await props.onSave(draft(mode));
    } finally {
      setSaving(false);
    }
  };
  const summary = () => publicCertSummary(props.status, props.now);

  return (
    <section class="glass cert">
      <h2>HTTPS de l'iPhone</h2>
      <div class="segmented three" role="radiogroup" aria-label="Certificat">
        {MODES.map((m) => (
          <button role="radio" aria-checked={props.config.mode === m.mode} disabled={saving()} onClick={() => props.config.mode !== m.mode && void save(m.mode)}>
            {m.label}
          </button>
        ))}
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

      <Show when={props.config.mode === "auto"}>
        <p class="hint">
          Rien à régler : PwaMote donne à ce PC un nom privé en <span class="mono">pwamote.app</span> qui pointe vers son adresse sur votre réseau, avec un
          vrai certificat. Le PC n'est jamais ouvert sur internet ; sans connexion, le certificat local prend le relais.
        </p>
      </Show>

      <Show when={props.config.mode === "custom"}>
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
          <button class="btn small" disabled={!dirty() || saving()} onClick={() => void save("custom")}>
            {saving() ? "Enregistrement…" : "Enregistrer"}
          </button>
        </div>
      </Show>
    </section>
  );
}
