import { Match, Show, Switch, createResource, createSignal, onCleanup, onMount } from "solid-js";
import { MAX_MESSAGE, canSend, emailOk, reportLang } from "../../client/src/lib/report";
import type { Backend } from "./ipc";

/** « Signaler un problème » : support PwaMote si ce PC est inscrit à pwamote.app, issue GitHub sinon. */
export function ReportDialog(props: { backend: Backend; onClose: () => void }) {
  const be = props.backend;
  const [info] = createResource(() => be.reportInfo());
  const [message, setMessage] = createSignal("");
  const [email, setEmail] = createSignal("");
  const [logs, setLogs] = createSignal(false);
  const [sending, setSending] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [ticket, setTicket] = createSignal<string | null>(null);

  const onKey = (e: KeyboardEvent) => e.key === "Escape" && !sending() && props.onClose();
  onMount(() => window.addEventListener("keydown", onKey));
  onCleanup(() => window.removeEventListener("keydown", onKey));

  const submit = async (e: Event) => {
    e.preventDefault();
    if (!canSend(message(), email()) || sending()) return;
    setSending(true);
    setError(null);
    try {
      const r = await be.sendReport({
        message: message().trim(),
        email: email().trim() || undefined,
        include_logs: logs(),
        lang: reportLang(navigator.language),
      });
      setTicket(r.ticket);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSending(false);
    }
  };
  const github = (label: string, primary = false) => (
    <button type="button" class="btn" classList={{ ghost: !primary }} onClick={() => void be.openIssue()}>
      {label}
    </button>
  );

  return (
    <div class="modal-back" onClick={(e) => e.target === e.currentTarget && !sending() && props.onClose()}>
      <section class="glass modal report" role="dialog" aria-modal="true" aria-labelledby="report-title">
        <h3 id="report-title">Signaler un problème</h3>
        <Switch>
          <Match when={info.loading}>
            <p class="hint">Un instant…</p>
          </Match>
          <Match when={info.error || info() === null}>
            <p class="hint">Le signalement n'est pas disponible. Vous pouvez écrire sur GitHub.</p>
            <div class="modal-actions">
              {github("Ouvrir GitHub", true)}
              <button class="btn ghost" onClick={props.onClose}>Fermer</button>
            </div>
          </Match>
          <Match when={ticket()}>
            <div class="report-done">
              <span class="report-check" aria-hidden="true">✓</span>
              <p>
                Merci, c'est envoyé. Référence <b class="mono">#{ticket()}</b>
              </p>
              <p class="hint">{email().trim() ? "La réponse arrivera par e-mail." : "Sans adresse e-mail, nous ne pourrons pas vous répondre, mais nous lirons tout."}</p>
              <div class="modal-actions">
                <button class="btn" onClick={props.onClose}>Fermer</button>
              </div>
            </div>
          </Match>
          <Match when={info()?.channel === "github"}>
            <p class="report-lead">Ce PC n'utilise pas le service pwamote.app : les problèmes se signalent sur GitHub, avec un compte gratuit.</p>
            <p class="hint">
              La version ({info()!.version}) et le système sont déjà remplis. Le ticket est public : n'y mettez ni adresse, ni nom de
              PC, ni capture montrant des données privées.
            </p>
            <div class="modal-actions">
              {github("Ouvrir GitHub", true)}
              <button class="btn ghost" onClick={props.onClose}>Annuler</button>
            </div>
          </Match>
          <Match when={info()?.channel === "cloud"}>
            <form class="report-form" onSubmit={submit}>
              <textarea
                rows={6}
                maxLength={MAX_MESSAGE}
                placeholder="Que se passe-t-il ? Ce que vous faisiez, ce que vous attendiez…"
                value={message()}
                onInput={(e) => setMessage(e.currentTarget.value)}
                autofocus
                required
              />
              <input
                type="email"
                placeholder="E-mail (facultatif, pour une réponse)"
                value={email()}
                aria-invalid={!emailOk(email())}
                onInput={(e) => setEmail(e.currentTarget.value)}
              />
              <label class="report-check-row">
                <input type="checkbox" checked={logs()} onChange={(e) => setLogs(e.currentTarget.checked)} />
                <span>
                  Joindre le journal de PwaMote
                  <small>
                    {logs()
                      ? "Adresses IP, e-mails, jetons et nom d'utilisateur sont retirés avant l'envoi."
                      : "Très utile pour les problèmes de connexion. Rien n'est joint sans votre accord."}
                  </small>
                </span>
              </label>
              <Show when={error()}>
                <p class="report-error" role="alert">
                  {error()}{" "}
                  <button type="button" class="linkish" onClick={() => void be.openIssue()}>
                    Signaler sur GitHub à la place
                  </button>
                </p>
              </Show>
              <p class="hint">
                Envoyé au support PwaMote avec la version {info()!.version}, sous {info()!.os}.
              </p>
              <div class="modal-actions">
                <button class="btn" type="submit" disabled={!canSend(message(), email()) || sending()}>
                  {sending() ? "Envoi…" : "Envoyer"}
                </button>
                <button class="btn ghost" type="button" disabled={sending()} onClick={props.onClose}>
                  Annuler
                </button>
              </div>
            </form>
          </Match>
        </Switch>
      </section>
    </div>
  );
}
