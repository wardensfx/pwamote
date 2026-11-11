import { Match, Show, Switch, createResource, createSignal, type JSX } from "solid-js";
import type { RemoteController } from "../lib/remote";
import { MAX_MESSAGE, canSend, emailOk, fetchReportInfo, phoneOs, reportLang, sendReport, withPhone } from "../lib/report";

/** Repli si le PC ne répond pas : le modèle d'issue du dépôt public, sans rien de prérempli. */
const GITHUB_FALLBACK = "https://github.com/wardensfx/pwamote/issues/new?template=bug.yml";

/** « Signaler un problème » : support PwaMote via le PC inscrit à pwamote.app, GitHub sinon. */
export function ReportSheet(props: { remote: RemoteController; onClose: () => void }): JSX.Element {
  const phone = phoneOs(navigator.userAgent, navigator.maxTouchPoints);
  const [info] = createResource(async () => {
    const token = props.remote.token;
    if (!token) throw new Error("Appairez d'abord ce téléphone avec le PC.");
    return fetchReportInfo(token);
  });
  const [message, setMessage] = createSignal("");
  const [email, setEmail] = createSignal("");
  const [logs, setLogs] = createSignal(false);
  const [sending, setSending] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [ticket, setTicket] = createSignal<string | null>(null);
  const issue = () => withPhone(info()?.issue_url ?? GITHUB_FALLBACK, phone);

  const submit = async (e: Event) => {
    e.preventDefault();
    const token = props.remote.token;
    if (!token || !canSend(message(), email()) || sending()) return;
    setSending(true);
    setError(null);
    try {
      setTicket(
        await sendReport(token, {
          message: message().trim(),
          email: email().trim() || undefined,
          include_logs: logs(),
          lang: reportLang(navigator.language),
          phone: phone || undefined,
        }),
      );
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSending(false);
    }
  };

  const github = (label: string) => (
    <a class="btn ghost small link" href={issue()} target="_blank" rel="noopener noreferrer">
      {label}
    </a>
  );

  return (
    <>
      <div class="sheet-back" onClick={props.onClose} />
      <div class="glass sheet report" role="dialog" aria-label="Signaler un problème">
        <div class="grab" />
        <h2>Signaler un problème</h2>
        <Switch>
          <Match when={info.loading}>
            <p class="note">Un instant…</p>
          </Match>
          <Match when={info.error}>
            <p class="note">{String(info.error?.message ?? info.error)}</p>
            <div class="row report-foot">{github("Signaler sur GitHub")}</div>
          </Match>
          <Match when={ticket()}>
            <div class="report-done">
              <span class="report-check" aria-hidden="true">✓</span>
              <p>
                Merci, c'est envoyé. Référence <b class="mono">#{ticket()}</b>
              </p>
              <p class="note">{email().trim() ? "La réponse arrivera par e-mail." : "Sans adresse e-mail, nous ne pourrons pas vous répondre, mais nous lirons tout."}</p>
              <button class="btn small" onClick={props.onClose}>
                Fermer
              </button>
            </div>
          </Match>
          <Match when={info()?.channel === "github"}>
            <p class="report-lead">Ce PC n'utilise pas le service pwamote.app : les problèmes se signalent sur GitHub, avec un compte gratuit.</p>
            <p class="note">
              La version ({info()!.version}) et le système sont déjà remplis. Le ticket est public : n'y mettez ni adresse, ni nom de PC, ni capture
              montrant des données privées.
            </p>
            <div class="row report-foot">
              {github("Ouvrir GitHub")}
              <button class="btn ghost small" onClick={props.onClose}>
                Annuler
              </button>
            </div>
          </Match>
          <Match when={info()?.channel === "cloud"}>
            <form class="report-form" onSubmit={submit}>
              <textarea
                class="kb-input report-text"
                rows={5}
                maxLength={MAX_MESSAGE}
                placeholder="Que se passe-t-il ? Ce que vous faisiez, ce que vous attendiez…"
                value={message()}
                onInput={(e) => setMessage(e.currentTarget.value)}
                required
              />
              <input
                class="kb-input"
                type="email"
                inputmode="email"
                autocomplete="email"
                placeholder="E-mail (facultatif, pour une réponse)"
                value={email()}
                aria-invalid={!emailOk(email())}
                onInput={(e) => setEmail(e.currentTarget.value)}
              />
              <label class="field tap">
                <span class="lbl">Joindre le journal du PC</span>
                <span class="switch">
                  <input type="checkbox" checked={logs()} onChange={(e) => setLogs(e.currentTarget.checked)} />
                  <i />
                </span>
              </label>
              <p class="note">
                {logs()
                  ? "Les adresses IP, e-mails, jetons et noms d'utilisateur sont retirés du journal avant l'envoi."
                  : "Le journal aide beaucoup pour les problèmes de connexion. Rien n'est joint sans votre accord."}
              </p>
              <Show when={error()}>
                <p class="report-error" role="alert">
                  {error()} <a href={issue()} target="_blank" rel="noopener noreferrer">Signaler sur GitHub à la place</a>
                </p>
              </Show>
              <div class="row report-foot">
                <button class="btn small" type="submit" disabled={!canSend(message(), email()) || sending()}>
                  {sending() ? "Envoi…" : "Envoyer"}
                </button>
                <button class="btn ghost small" type="button" onClick={props.onClose}>
                  Annuler
                </button>
              </div>
              <p class="note">
                Envoyé au support PwaMote par le PC, avec la version {info()!.version}, le système du PC{phone ? ` et celui du téléphone (${phone})` : ""}.
              </p>
            </form>
          </Match>
        </Switch>
      </div>
    </>
  );
}
