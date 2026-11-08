import { For, Show, createSignal, onMount, type JSX } from "solid-js";
import type { ActionInput } from "../generated/ActionInput";
import type { ButtonView } from "../generated/ButtonView";
import type { InputValue } from "../generated/InputValue";
import { finalValues, formatStep, initialValues, isValid, RememberedValues, type Values } from "../lib/actions";
import type { RemoteController } from "../lib/remote";

const remembered = new RememberedValues();
const PAGE_KEY = "pwamote.deck-page";

function savedPage(): number {
  try {
    return Number(localStorage.getItem(PAGE_KEY) ?? 0) || 0;
  } catch {
    return 0;
  }
}

/** Panneau de boutons façon Stream Deck : une page par écran, on glisse pour changer. */
export function ActionPanel(props: { remote: RemoteController }) {
  const r = props.remote;
  const [page, setPage] = createSignal(0);
  const [form, setForm] = createSignal<ButtonView | null>(null);
  let scroller!: HTMLDivElement;

  const pages = () => r.deck()?.pages ?? [];

  onMount(() => {
    const start = Math.min(savedPage(), Math.max(0, pages().length - 1));
    if (start > 0) {
      scroller.scrollLeft = start * scroller.clientWidth;
      setPage(start);
    }
  });

  const onScroll = () => {
    const i = Math.round(scroller.scrollLeft / Math.max(1, scroller.clientWidth));
    if (i !== page()) {
      setPage(i);
      try {
        localStorage.setItem(PAGE_KEY, String(i));
      } catch {
        /* navigation privée */
      }
    }
  };

  const goTo = (i: number) => scroller.scrollTo({ left: i * scroller.clientWidth, behavior: "smooth" });

  const press = (b: ButtonView) => {
    if (b.inputs.length > 0) setForm(b);
    else r.runAction(b.id);
  };

  return (
    <div class="glass deck">
      <Show
        when={pages().length > 0}
        fallback={
          <p class="deck-empty">
            {r.deck() ? "Aucune action pour l'instant. Créez-en dans la fenêtre PwaMote du PC, onglet Actions." : "Chargement des actions…"}
          </p>
        }
      >
        <div class="deck-head">
          <span class="deck-name">{pages()[Math.min(page(), pages().length - 1)]?.name}</span>
          <Show when={pages().length > 1}>
            <div class="deck-dots" role="tablist">
              <For each={pages()}>
                {(p, i) => <button role="tab" aria-label={p.name} aria-selected={i() === page()} onClick={() => goTo(i())} />}
              </For>
            </div>
          </Show>
        </div>
        <div class="deck-pages" ref={scroller} onScroll={onScroll}>
          <For each={pages()}>
            {(p) => (
              <div class="deck-page" style={{ "grid-template-columns": `repeat(${p.columns}, 1fr)`, "grid-template-rows": `repeat(${p.rows}, 1fr)` }}>
                <For each={p.cells}>
                  {(cell) => (
                    <Show when={cell} fallback={<span class="act-empty" />}>
                      {(b) => (
                        <button
                          class="act"
                          data-color={b().color}
                          data-state={r.actionStates()[b().id] ?? "idle"}
                          data-compact={p.columns >= 4 || undefined}
                          onClick={() => press(b())}
                          onContextMenu={(e) => e.preventDefault()}
                        >
                          <Show when={b().icon}>
                            <span class="act-icon">{b().icon}</span>
                          </Show>
                          <span class="act-label">{b().label}</span>
                          <Show when={b().inputs.length > 0}>
                            <span class="act-more" aria-hidden="true">•••</span>
                          </Show>
                        </button>
                      )}
                    </Show>
                  )}
                </For>
              </div>
            )}
          </For>
        </div>
      </Show>
      <Show when={form()}>{(b) => <ActionForm remote={r} button={b()} onClose={() => setForm(null)} />}</Show>
    </div>
  );
}

function Field(props: { input: ActionInput; value: InputValue | undefined; onChange: (v: InputValue) => void; onSubmit: () => void }): JSX.Element {
  // Le type d'un champ ne change pas pendant que le formulaire est ouvert : un simple aiguillage suffit.
  const k = props.input.kind;
  const num = () => Number(String(props.value).replace(",", "."));
  switch (k.type) {
    case "choice":
      return (
        <div class="field col">
          <span class="lbl">{props.input.label}</span>
          <Show
            when={k.options.length <= 4}
            fallback={
              <select class="select" value={String(props.value ?? "")} onChange={(e) => props.onChange(e.currentTarget.value)}>
                <For each={k.options}>{(o) => <option value={o.value}>{o.label}</option>}</For>
              </select>
            }
          >
            <div class="seg" role="radiogroup">
              <For each={k.options}>
                {(o) => (
                  <button role="radio" aria-checked={props.value === o.value} onClick={() => props.onChange(o.value)}>
                    {o.label}
                  </button>
                )}
              </For>
            </div>
          </Show>
        </div>
      );
    case "slider":
      return (
        <div class="field col">
          <div class="head">
            <span class="lbl">{props.input.label}</span>
            <span class="val">
              {formatStep(Number(props.value ?? k.default), k.step)}
              {k.unit ? ` ${k.unit}` : ""}
            </span>
          </div>
          <input type="range" min={k.min} max={k.max} step={k.step} value={Number(props.value ?? k.default)} onInput={(e) => props.onChange(parseFloat(e.currentTarget.value))} />
        </div>
      );
    case "number":
      return (
        <div class="field">
          <span class="lbl">{props.input.label}</span>
          <div class="stepper">
            <button aria-label="Moins" onClick={() => props.onChange(clampNum(num() - 1, k.min, k.max))}>
              −
            </button>
            <input
              class="num"
              inputmode={k.integer ? "numeric" : "decimal"}
              value={String(props.value ?? "").replace(".", ",")}
              aria-invalid={!isValid(props.input, props.value)}
              onInput={(e) => props.onChange(e.currentTarget.value)}
              onKeyDown={(e) => e.key === "Enter" && props.onSubmit()}
            />
            <button aria-label="Plus" onClick={() => props.onChange(clampNum(num() + 1, k.min, k.max))}>
              +
            </button>
          </div>
        </div>
      );
    case "text":
      return (
        <div class="field col">
          <span class="lbl">{props.input.label}</span>
          <input
            class="text-in"
            type="text"
            placeholder={k.placeholder}
            value={String(props.value ?? "")}
            autocomplete="off"
            enterkeyhint="go"
            onInput={(e) => props.onChange(e.currentTarget.value)}
            onKeyDown={(e) => e.key === "Enter" && props.onSubmit()}
          />
        </div>
      );
    case "toggle":
      return (
        <label class="field tap">
          <span class="lbl">{props.input.label}</span>
          <span class="switch">
            <input type="checkbox" checked={props.value === true} onChange={(e) => props.onChange(e.currentTarget.checked)} />
            <i />
          </span>
        </label>
      );
  }
}

function clampNum(v: number, min: number | null, max: number | null): number {
  const n = Number.isFinite(v) ? v : (min ?? 0);
  return Math.min(max ?? Infinity, Math.max(min ?? -Infinity, n));
}

/** Formulaire demandé avant de lancer une action qui attend des valeurs. */
function ActionForm(props: { remote: RemoteController; button: ButtonView; onClose: () => void }) {
  const b = props.button;
  const [values, setValues] = createSignal<Values>(initialValues(b.inputs, remembered.get(b.id)));
  const valid = () => b.inputs.every((i) => isValid(i, values()[i.id]));

  const launch = () => {
    if (!valid()) return;
    const v = finalValues(b.inputs, values());
    remembered.set(b.id, v);
    props.remote.runAction(b.id, v);
    props.onClose();
  };

  return (
    <>
      <div class="sheet-back" onClick={props.onClose} />
      <div class="glass sheet action-form" role="dialog" aria-label={b.label}>
        <div class="grab" />
        <div class="form-head" data-color={b.color}>
          <Show when={b.icon}>
            <span class="act-icon">{b.icon}</span>
          </Show>
          <h2>{b.label}</h2>
        </div>
        <For each={b.inputs}>
          {(input) => <Field input={input} value={values()[input.id]} onChange={(v) => setValues({ ...values(), [input.id]: v })} onSubmit={launch} />}
        </For>
        <div class="form-actions">
          <button class="btn ghost" onClick={props.onClose}>
            Annuler
          </button>
          <button class="btn" disabled={!valid()} onClick={launch}>
            Lancer
          </button>
        </div>
      </div>
    </>
  );
}
