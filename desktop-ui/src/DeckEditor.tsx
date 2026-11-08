import { For, Show, createMemo, createSignal, onMount, type JSX } from "solid-js";
import { createStore, produce, reconcile } from "solid-js/store";
import {
  CHAR_KEYS,
  GRID,
  INPUT_KINDS,
  MODIFIERS,
  NAMED_KEYS,
  STEP_KINDS,
  buttonProblems,
  clone,
  keyFromEvent,
  modifierLabel,
  modifiersFromEvent,
  move,
  newButton,
  newInput,
  newInputKind,
  newPage,
  newStep,
  occupiedExtent,
  resizePage,
  shortcutLabel,
  swapCells,
  toFieldId,
  unknownFields,
  type ActionButton,
  type ActionInput,
  type ActionStep,
  type Deck,
  type Key,
} from "./deck";
import type { Backend } from "./ipc";

const COLORS: [ActionButton["color"], string][] = [
  ["glass", "Verre"],
  ["amber", "Ambre"],
  ["red", "Rouge"],
  ["green", "Vert"],
  ["blue", "Bleu"],
  ["violet", "Violet"],
];
const ICONS = ["⏯", "⏩", "⏪", "🐢", "🐇", "🔊", "🔇", "🎬", "🎵", "🔎", "📋", "✂", "🖥", "💡", "⚙", "🚀", "⭐", "🌙"];

function Stepper(props: { value: number; min: number; max: number; onChange: (v: number) => void; label: string }) {
  return (
    <div class="stepper" aria-label={props.label}>
      <button class="btn ghost tiny" disabled={props.value <= props.min} onClick={() => props.onChange(props.value - 1)} aria-label={`${props.label} moins`}>
        −
      </button>
      <span class="stepper-val">{props.value}</span>
      <button class="btn ghost tiny" disabled={props.value >= props.max} onClick={() => props.onChange(props.value + 1)} aria-label={`${props.label} plus`}>
        +
      </button>
    </div>
  );
}

function NumberField(props: { value: number | null; onChange: (v: number | null) => void; placeholder?: string; optional?: boolean; label: string }) {
  return (
    <input
      class="field num"
      type="number"
      aria-label={props.label}
      placeholder={props.placeholder}
      value={props.value ?? ""}
      onChange={(e) => {
        const raw = e.currentTarget.value.trim().replace(",", ".");
        if (raw === "") return props.onChange(props.optional ? null : 0);
        const n = Number(raw);
        if (Number.isFinite(n)) props.onChange(n);
      }}
    />
  );
}

function Labeled(props: { label: string; hint?: string; children: JSX.Element; grow?: boolean }) {
  return (
    <label class="labeled" classList={{ grow: props.grow }}>
      <span class="labeled-title">{props.label}</span>
      {props.children}
      <Show when={props.hint}>
        <span class="labeled-hint">{props.hint}</span>
      </Show>
    </label>
  );
}

/** Raccourci : on le tape au clavier du PC, ou on le choisit dans la liste. */
function KeysFields(props: { step: Extract<ActionStep, { kind: "keys" }>; edit: (fn: (s: Extract<ActionStep, { kind: "keys" }>) => void) => void }) {
  const [listening, setListening] = createSignal(false);
  return (
    <>
      <div
        class="capture"
        tabindex="0"
        data-listening={listening()}
        onFocus={() => setListening(true)}
        onBlur={() => setListening(false)}
        onKeyDown={(e) => {
          if (e.key === "Tab" && !e.ctrlKey && !e.altKey) return; // garder la navigation au clavier
          e.preventDefault();
          const key = keyFromEvent(e);
          if (!key) return;
          const modifiers = modifiersFromEvent(e);
          props.edit((s) => {
            s.key = key;
            s.modifiers = modifiers;
          });
          e.currentTarget.blur();
        }}
      >
        <span class="capture-keys">{shortcutLabel(props.step.key, props.step.modifiers)}</span>
        <span class="capture-hint">{listening() ? "Appuyez sur la touche ou le raccourci…" : "Cliquez puis tapez le raccourci"}</span>
      </div>
      <div class="inline">
        <select
          aria-label="Touche"
          value={props.step.key as string}
          onChange={(e) =>
            props.edit((s) => {
              s.key = e.currentTarget.value as Key;
            })
          }
        >
          <optgroup label="Lettres et chiffres">
            <For each={CHAR_KEYS}>{(k) => <option value={k as string}>{String(k).toUpperCase()}</option>}</For>
          </optgroup>
          <optgroup label="Touches spéciales">
            <For each={NAMED_KEYS}>{([k, label]) => <option value={k as string}>{label}</option>}</For>
          </optgroup>
        </select>
        <div class="mods">
          <For each={MODIFIERS}>
            {(m) => (
              <button
                class="mod"
                aria-pressed={props.step.modifiers.includes(m)}
                onClick={() =>
                  props.edit((s) => {
                    s.modifiers = s.modifiers.includes(m) ? s.modifiers.filter((x) => x !== m) : MODIFIERS.filter((x) => x === m || s.modifiers.includes(x));
                  })
                }
              >
                {modifierLabel(m)}
              </button>
            )}
          </For>
        </div>
      </div>
      <p class="hint">Envoyée comme une vraie touche du clavier : les raccourcis et les extensions du navigateur la reçoivent.</p>
    </>
  );
}

function StepFields(props: { step: ActionStep; edit: (fn: (s: ActionStep) => void) => void }) {
  const s = props.step;
  switch (s.kind) {
    case "keys":
      return <KeysFields step={s} edit={props.edit as never} />;
    case "text":
      return (
        <Labeled label="Texte" hint="Tapé tel quel, accents et emoji compris. {champ} insère une valeur saisie sur le téléphone.">
          <textarea class="field" rows="2" value={s.text} onInput={(e) => props.edit((x) => x.kind === "text" && (x.text = e.currentTarget.value))} />
        </Labeled>
      );
    case "run":
      return (
        <>
          <Labeled label="Programme" hint="Chemin complet ou nom d'un programme du PATH : powershell.exe, C:\Outils\script.bat…">
            <input class="field mono" value={s.program} spellcheck={false} onInput={(e) => props.edit((x) => x.kind === "run" && (x.program = e.currentTarget.value))} />
          </Labeled>
          <Labeled label="Arguments" hint="Un argument par ligne, transmis tels quels, sans interpréteur de commandes. {champ} est remplacé par la valeur, qui reste un seul argument.">
            <textarea
              class="field mono"
              rows={Math.max(2, s.args.length + 1)}
              spellcheck={false}
              value={s.args.join("\n")}
              onInput={(e) => props.edit((x) => x.kind === "run" && (x.args = e.currentTarget.value.split("\n").filter((a, i, all) => a !== "" || i < all.length - 1)))}
            />
          </Labeled>
          <Labeled label="Dossier de travail (facultatif)">
            <input class="field mono" value={s.cwd} spellcheck={false} onInput={(e) => props.edit((x) => x.kind === "run" && (x.cwd = e.currentTarget.value))} />
          </Labeled>
          <label class="check">
            <input type="checkbox" checked={s.hidden} onChange={(e) => props.edit((x) => x.kind === "run" && (x.hidden = e.currentTarget.checked))} />
            Sans fenêtre (scripts, outils en ligne de commande)
          </label>
        </>
      );
    case "open":
      return (
        <Labeled label="Adresse, fichier ou dossier" hint="Ouvert avec l'application par défaut. {champ:url} encode la valeur pour une adresse web.">
          <input class="field mono" value={s.target} spellcheck={false} onInput={(e) => props.edit((x) => x.kind === "open" && (x.target = e.currentTarget.value))} />
        </Labeled>
      );
    case "delay":
      return (
        <Labeled label="Durée (ms)" hint="Jusqu'à 10 000 ms.">
          <NumberField label="Durée" value={s.ms} onChange={(v) => props.edit((x) => x.kind === "delay" && (x.ms = Math.max(0, Math.min(10000, Math.round(v ?? 0)))))} />
        </Labeled>
      );
  }
}

function InputKindFields(props: { input: ActionInput; edit: (fn: (i: ActionInput) => void) => void }) {
  const k = props.input.kind;
  const set = <T extends ActionInput["kind"]["type"]>(type: T, fn: (k: Extract<ActionInput["kind"], { type: T }>) => void) =>
    props.edit((i) => {
      if (i.kind.type === type) fn(i.kind as never);
    });
  switch (k.type) {
    case "choice":
      return (
        <>
          <div class="options">
            <div class="options-head">
              <span>Libellé affiché</span>
              <span>Valeur insérée</span>
            </div>
            <For each={k.options}>
              {(o, i) => (
                <div class="option">
                  <input class="field" value={o.label} aria-label="Libellé" onInput={(e) => set("choice", (c) => (c.options[i()]!.label = e.currentTarget.value))} />
                  <input
                    class="field mono"
                    value={o.value}
                    aria-label="Valeur"
                    onInput={(e) =>
                      set("choice", (c) => {
                        if (c.default === c.options[i()]!.value) c.default = e.currentTarget.value;
                        c.options[i()]!.value = e.currentTarget.value;
                      })
                    }
                  />
                  <button class="icon-btn" aria-label="Retirer l'option" onClick={() => set("choice", (c) => c.options.splice(i(), 1))}>
                    ×
                  </button>
                </div>
              )}
            </For>
            <button
              class="btn ghost tiny"
              onClick={() => set("choice", (c) => c.options.push({ label: `Option ${c.options.length + 1}`, value: String(c.options.length + 1) }))}
            >
              + Option
            </button>
          </div>
          <Labeled label="Choix par défaut">
            <select value={k.default} onChange={(e) => set("choice", (c) => (c.default = e.currentTarget.value))}>
              <For each={k.options}>{(o) => <option value={o.value}>{o.label}</option>}</For>
            </select>
          </Labeled>
        </>
      );
    case "slider":
      return (
        <div class="grid4">
          <Labeled label="Min">
            <NumberField label="Min" value={k.min} onChange={(v) => set("slider", (s) => (s.min = v ?? 0))} />
          </Labeled>
          <Labeled label="Max">
            <NumberField label="Max" value={k.max} onChange={(v) => set("slider", (s) => (s.max = v ?? 0))} />
          </Labeled>
          <Labeled label="Pas">
            <NumberField label="Pas" value={k.step} onChange={(v) => set("slider", (s) => (s.step = v ?? 1))} />
          </Labeled>
          <Labeled label="Défaut">
            <NumberField label="Défaut" value={k.default} onChange={(v) => set("slider", (s) => (s.default = v ?? 0))} />
          </Labeled>
          <Labeled label="Unité">
            <input class="field" value={k.unit} placeholder="%, s, ×" onInput={(e) => set("slider", (s) => (s.unit = e.currentTarget.value))} />
          </Labeled>
        </div>
      );
    case "number":
      return (
        <div class="grid4">
          <Labeled label="Min">
            <NumberField label="Min" optional placeholder="aucun" value={k.min} onChange={(v) => set("number", (n) => (n.min = v))} />
          </Labeled>
          <Labeled label="Max">
            <NumberField label="Max" optional placeholder="aucun" value={k.max} onChange={(v) => set("number", (n) => (n.max = v))} />
          </Labeled>
          <Labeled label="Défaut">
            <NumberField label="Défaut" value={k.default} onChange={(v) => set("number", (n) => (n.default = v ?? 0))} />
          </Labeled>
          <label class="check">
            <input type="checkbox" checked={k.integer} onChange={(e) => set("number", (n) => (n.integer = e.currentTarget.checked))} />
            Entier
          </label>
        </div>
      );
    case "text":
      return (
        <div class="grid2">
          <Labeled label="Valeur par défaut">
            <input class="field" value={k.default} onInput={(e) => set("text", (t) => (t.default = e.currentTarget.value))} />
          </Labeled>
          <Labeled label="Texte d'aide">
            <input class="field" value={k.placeholder} onInput={(e) => set("text", (t) => (t.placeholder = e.currentTarget.value))} />
          </Labeled>
        </div>
      );
    case "toggle":
      return (
        <div class="grid4">
          <Labeled label="Si activé">
            <input class="field mono" value={k.on} onInput={(e) => set("toggle", (t) => (t.on = e.currentTarget.value))} />
          </Labeled>
          <Labeled label="Si désactivé">
            <input class="field mono" value={k.off} onInput={(e) => set("toggle", (t) => (t.off = e.currentTarget.value))} />
          </Labeled>
          <label class="check">
            <input type="checkbox" checked={k.default} onChange={(e) => set("toggle", (t) => (t.default = e.currentTarget.checked))} />
            Activé par défaut
          </label>
        </div>
      );
  }
}

export function DeckEditor(props: { backend: Backend; onDirty?: (dirty: boolean) => void }) {
  const be = props.backend;
  const [saved, setSaved] = createSignal<Deck | null>(null);
  const [draft, setDraft] = createStore<Deck>({ pages: [] });
  const [pageIx, setPageIx] = createSignal(0);
  const [cellIx, setCellIx] = createSignal<number | null>(null);
  // Glisser-déposer aux événements de pointeur : le glisser HTML5 est intercepté par la fenêtre
  // Tauri (dépôt de fichiers) et ne fonctionne pas dans WebView2.
  const [dragFrom, setDragFrom] = createSignal<number | null>(null);
  const [dragOver, setDragOver] = createSignal<number | null>(null);
  const [dragging, setDragging] = createSignal(false);
  let justDropped = false;
  const [busy, setBusy] = createSignal(false);
  const [test, setTest] = createSignal<{ id: string; state: "running" | "ok" | "error"; message?: string } | null>(null);
  const [error, setError] = createSignal<string | null>(null);

  onMount(async () => {
    try {
      const d = await be.getDeck();
      setSaved(d);
      setDraft(reconcile(clone(d)));
    } catch (e) {
      setError(String(e));
    }
  });

  const dirty = createMemo(() => {
    const s = saved();
    const d = s !== null && JSON.stringify(draft) !== JSON.stringify(s);
    props.onDirty?.(d);
    return d;
  });

  const page = () => draft.pages[pageIx()];
  const button = () => {
    const i = cellIx();
    return i === null ? null : (page()?.cells[i] ?? null);
  };

  const editDeck = (fn: (d: Deck) => void) => setDraft(produce(fn));
  const editPage = (fn: (p: Deck["pages"][number]) => void) => editDeck((d) => d.pages[pageIx()] && fn(d.pages[pageIx()]!));
  const editButton = (fn: (b: ActionButton) => void) =>
    editPage((p) => {
      const i = cellIx();
      const b = i === null ? null : p.cells[i];
      if (b) fn(b);
    });
  const replacePage = (fn: (p: Deck["pages"][number]) => Deck["pages"][number]) =>
    editDeck((d) => {
      const p = d.pages[pageIx()];
      if (p) d.pages[pageIx()] = fn(clone(p));
    });

  const addPage = () => {
    if (draft.pages.length >= GRID.maxPages) return;
    editDeck((d) => d.pages.push(newPage(d.pages.length)));
    setPageIx(draft.pages.length - 1);
    setCellIx(null);
  };

  const removePage = () => {
    editDeck((d) => d.pages.splice(pageIx(), 1));
    setPageIx(Math.max(0, Math.min(pageIx(), draft.pages.length - 1)));
    setCellIx(null);
  };

  const movePage = (delta: number) => {
    const to = pageIx() + delta;
    if (to < 0 || to >= draft.pages.length) return;
    setDraft("pages", move([...draft.pages.map(clone)], pageIx(), to));
    setPageIx(to);
  };

  const onCell = (i: number) => {
    if (!page()?.cells[i]) editPage((p) => (p.cells[i] = newButton()));
    setCellIx(i);
  };

  const cellAt = (x: number, y: number): number | null => {
    const el = document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-cell]");
    return el ? Number(el.dataset.cell) : null;
  };

  const startDrag = (from: number, down: PointerEvent) => {
    setDragFrom(from);
    setDragOver(from);
    const move = (e: PointerEvent) => {
      if (!dragging() && Math.hypot(e.clientX - down.clientX, e.clientY - down.clientY) < 6) return;
      setDragging(true);
      setDragOver(cellAt(e.clientX, e.clientY));
    };
    const up = (e: PointerEvent) => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", up);
      const to = e.type === "pointerup" ? cellAt(e.clientX, e.clientY) : null;
      if (dragging()) {
        justDropped = true;
        setTimeout(() => (justDropped = false), 0);
        if (to !== null && to !== from) {
          replacePage((x) => swapCells(x, from, to));
          if (cellIx() === from) setCellIx(to);
          else if (cellIx() === to) setCellIx(from);
        }
      }
      setDragging(false);
      setDragFrom(null);
      setDragOver(null);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", up);
  };

  const save = async (): Promise<boolean> => {
    setBusy(true);
    setError(null);
    try {
      const result = await be.setDeck(clone(draft));
      setSaved(result);
      setDraft(reconcile(clone(result)));
      return true;
    } catch (e) {
      setError(`Enregistrement impossible : ${e}`);
      return false;
    } finally {
      setBusy(false);
    }
  };

  const revert = () => {
    const s = saved();
    if (s) setDraft(reconcile(clone(s)));
    setPageIx((i) => Math.min(i, Math.max(0, (s?.pages.length ?? 1) - 1)));
    setCellIx(null);
  };

  const runTest = async () => {
    const b = button();
    if (!b) return;
    if (dirty() && !(await save())) return;
    setTest({ id: b.id, state: "running" });
    try {
      await be.testAction(b.id);
      setTest({ id: b.id, state: "ok" });
    } catch (e) {
      setTest({ id: b.id, state: "error", message: String(e) });
    }
    setTimeout(() => setTest((t) => (t?.state === "running" ? t : null)), 4000);
  };

  const fields = () => (button()?.inputs ?? []).map((i) => i.id);

  return (
    <div class="deck-editor">
      <Show when={saved()} fallback={<div class="loading">{error() ?? "Chargement des actions…"}</div>}>
        <section class="glass">
          <div class="pages-bar" role="tablist">
            <For each={draft.pages}>
              {(p, i) => (
                <button
                  class="page-tab"
                  role="tab"
                  aria-selected={i() === pageIx()}
                  onClick={() => {
                    setPageIx(i());
                    setCellIx(null);
                  }}
                >
                  {p.name || `Page ${i() + 1}`}
                </button>
              )}
            </For>
            <button class="page-tab add" onClick={addPage} disabled={draft.pages.length >= GRID.maxPages} aria-label="Ajouter une page">
              + Page
            </button>
          </div>

          <Show
            when={page()}
            fallback={
              <p class="empty">
                Aucune page. Ajoutez-en une : elle apparaîtra sur le téléphone, dans le panneau <b>Actions</b>.
              </p>
            }
          >
            {(p) => (
              <>
                <div class="page-settings">
                  <Labeled label="Nom de la page" grow>
                    <input class="field" value={p().name} maxLength={40} onInput={(e) => editPage((x) => (x.name = e.currentTarget.value))} />
                  </Labeled>
                  <Labeled label="Colonnes">
                    <Stepper
                      label="Colonnes"
                      value={p().columns}
                      min={Math.max(GRID.minColumns, occupiedExtent(p()).columns)}
                      max={GRID.maxColumns}
                      onChange={(c) => replacePage((x) => resizePage(x, c, x.rows))}
                    />
                  </Labeled>
                  <Labeled label="Lignes">
                    <Stepper
                      label="Lignes"
                      value={p().rows}
                      min={Math.max(GRID.minRows, occupiedExtent(p()).rows)}
                      max={GRID.maxRows}
                      onChange={(r) => replacePage((x) => resizePage(x, x.columns, r))}
                    />
                  </Labeled>
                </div>

                <div class="grid-wrap">
                  <div class="grid" style={{ "grid-template-columns": `repeat(${p().columns}, 1fr)` }}>
                    <For each={p().cells}>
                      {(cell, i) => (
                        <button
                          class="cell"
                          classList={{
                            filled: !!cell,
                            selected: cellIx() === i(),
                            dragging: dragging() && dragFrom() === i(),
                            over: dragging() && dragOver() === i() && dragFrom() !== i(),
                          }}
                          data-cell={i()}
                          data-color={cell?.color}
                          onPointerDown={(e) => cell && e.button === 0 && startDrag(i(), e)}
                          onClick={() => {
                            // Un glisser qui vient de se terminer n'est pas un clic.
                            if (justDropped) return;
                            onCell(i());
                          }}
                          title={cell ? "Cliquer pour modifier · glisser pour déplacer" : "Ajouter une action ici"}
                        >
                          <Show when={cell} fallback={<span class="cell-add">+</span>}>
                            {(b) => (
                              <>
                                <Show when={b().icon}>
                                  <span class="cell-icon">{b().icon}</span>
                                </Show>
                                <span class="cell-label">{b().label || "Sans nom"}</span>
                                <Show when={buttonProblems(b()).length > 0}>
                                  <span class="cell-warn" title="À compléter">!</span>
                                </Show>
                              </>
                            )}
                          </Show>
                        </button>
                      )}
                    </For>
                  </div>
                  <p class="hint">Cliquez une case vide pour créer une action ; glissez un bouton pour le déplacer. Sur le téléphone, on passe d'une page à l'autre en glissant.</p>
                </div>

                <div class="page-actions">
                  <button class="btn ghost small" onClick={() => movePage(-1)} disabled={pageIx() === 0}>
                    ← Déplacer
                  </button>
                  <button class="btn ghost small" onClick={() => movePage(1)} disabled={pageIx() >= draft.pages.length - 1}>
                    Déplacer →
                  </button>
                  <span class="spacer" />
                  <button class="btn danger small" onClick={removePage}>
                    Supprimer la page
                  </button>
                </div>
              </>
            )}
          </Show>
        </section>

        <Show when={button()}>
          {(b) => (
            <section class="glass editor">
              <div class="editor-head">
                <div class="preview" data-color={b().color}>
                  <Show when={b().icon}>
                    <span class="cell-icon">{b().icon}</span>
                  </Show>
                  <span class="cell-label">{b().label || "Sans nom"}</span>
                </div>
                <div class="grow">
                  <h2>Bouton</h2>
                  <div class="grid2">
                    <Labeled label="Libellé">
                      <input class="field" value={b().label} maxLength={40} onInput={(e) => editButton((x) => (x.label = e.currentTarget.value))} />
                    </Labeled>
                    <Labeled label="Icône (emoji ou 1–2 lettres)">
                      <input class="field" value={b().icon} maxLength={8} onInput={(e) => editButton((x) => (x.icon = e.currentTarget.value))} />
                    </Labeled>
                  </div>
                </div>
              </div>
              <div class="icons">
                <For each={ICONS}>
                  {(ic) => (
                    <button class="icon-pick" aria-pressed={b().icon === ic} onClick={() => editButton((x) => (x.icon = ic))}>
                      {ic}
                    </button>
                  )}
                </For>
              </div>
              <div class="swatches" role="radiogroup" aria-label="Couleur">
                <For each={COLORS}>
                  {([c, label]) => <button class="swatch" data-color={c} role="radio" aria-checked={b().color === c} title={label} aria-label={label} onClick={() => editButton((x) => (x.color = c))} />}
                </For>
              </div>

              <h3>Étapes</h3>
              <ol class="steps">
                <For each={b().steps}>
                  {(step, i) => (
                    <li class="step">
                      <div class="step-head">
                        <span class="step-n">{i() + 1}</span>
                        <select
                          aria-label="Type d'étape"
                          value={step.kind}
                          onChange={(e) => editButton((x) => (x.steps[i()] = newStep(e.currentTarget.value as ActionStep["kind"])))}
                        >
                          <For each={STEP_KINDS}>{([k, label]) => <option value={k}>{label}</option>}</For>
                        </select>
                        <span class="spacer" />
                        <button class="icon-btn" aria-label="Monter" disabled={i() === 0} onClick={() => editButton((x) => (x.steps = move(x.steps.map(clone), i(), i() - 1)))}>
                          ↑
                        </button>
                        <button
                          class="icon-btn"
                          aria-label="Descendre"
                          disabled={i() === b().steps.length - 1}
                          onClick={() => editButton((x) => (x.steps = move(x.steps.map(clone), i(), i() + 1)))}
                        >
                          ↓
                        </button>
                        <button class="icon-btn" aria-label="Supprimer l'étape" onClick={() => editButton((x) => x.steps.splice(i(), 1))}>
                          ×
                        </button>
                      </div>
                      <Show when={step.kind} keyed>
                        {(_kind) => <StepFields step={step} edit={(fn) => editButton((x) => fn(x.steps[i()]!))} />}
                      </Show>
                    </li>
                  )}
                </For>
              </ol>
              <div class="add-row">
                <For each={STEP_KINDS}>
                  {([k, label]) => (
                    <button class="btn ghost tiny" onClick={() => editButton((x) => x.steps.push(newStep(k)))} disabled={b().steps.length >= 32}>
                      + {label}
                    </button>
                  )}
                </For>
              </div>

              <h3>Champs demandés sur le téléphone</h3>
              <p class="hint">
                Facultatif. Avant de lancer l'action, le téléphone affiche un formulaire ; chaque valeur s'insère dans les étapes avec <span class="mono">{"{nom}"}</span>.
              </p>
              <ol class="steps">
                <For each={b().inputs}>
                  {(input, i) => (
                    <li class="step">
                      <div class="step-head">
                        <select
                          aria-label="Type de champ"
                          value={input.kind.type}
                          onChange={(e) => editButton((x) => (x.inputs[i()]!.kind = newInputKind(e.currentTarget.value as ActionInput["kind"]["type"])))}
                        >
                          <For each={INPUT_KINDS}>{([k, label]) => <option value={k}>{label}</option>}</For>
                        </select>
                        <span class="mono tag">{`{${input.id}}`}</span>
                        <span class="spacer" />
                        <button class="icon-btn" aria-label="Supprimer le champ" onClick={() => editButton((x) => x.inputs.splice(i(), 1))}>
                          ×
                        </button>
                      </div>
                      <div class="grid2">
                        <Labeled label="Libellé sur le téléphone">
                          <input
                            class="field"
                            value={input.label}
                            onInput={(e) => {
                              const label = e.currentTarget.value;
                              editButton((x) => {
                                const it = x.inputs[i()]!;
                                // Le nom suit le libellé tant qu'on ne l'a pas personnalisé.
                                if (it.id === toFieldId(it.label) || /^champ\d+$/.test(it.id)) {
                                  const id = toFieldId(label);
                                  if (id && !x.inputs.some((o, j) => j !== i() && o.id === id)) it.id = id;
                                }
                                it.label = label;
                              });
                            }}
                          />
                        </Labeled>
                        <Labeled label="Nom dans les étapes">
                          <input
                            class="field mono"
                            value={input.id}
                            spellcheck={false}
                            onInput={(e) => editButton((x) => (x.inputs[i()]!.id = e.currentTarget.value.replace(/[^a-zA-Z0-9_]/g, "")))}
                          />
                        </Labeled>
                      </div>
                      <Show when={input.kind.type} keyed>
                        {(_type) => <InputKindFields input={input} edit={(fn) => editButton((x) => fn(x.inputs[i()]!))} />}
                      </Show>
                    </li>
                  )}
                </For>
              </ol>
              <button class="btn ghost tiny" disabled={b().inputs.length >= 8} onClick={() => editButton((x) => x.inputs.push(newInput(fields())))}>
                + Champ
              </button>

              <Show when={buttonProblems(b()).length > 0 || unknownFields(b()).length > 0}>
                <ul class="problems">
                  <For each={buttonProblems(b())}>{(p) => <li>{p}</li>}</For>
                  <For each={unknownFields(b())}>{(f) => <li>{`{${f}} n'est pas un champ déclaré : il sera laissé tel quel.`}</li>}</For>
                </ul>
              </Show>

              <div class="editor-foot">
                <button class="btn small" onClick={runTest} disabled={busy() || test()?.state === "running"}>
                  {dirty() ? "Enregistrer et tester" : "Tester sur ce PC"}
                </button>
                <Show when={test() && test()!.id === b().id}>
                  <span class="test-result" data-state={test()!.state}>
                    {test()!.state === "running" ? "En cours…" : test()!.state === "ok" ? "Exécutée" : test()!.message}
                  </span>
                </Show>
                <span class="spacer" />
                <button
                  class="btn danger small"
                  onClick={() => {
                    editPage((p) => (p.cells[cellIx()!] = null));
                    setCellIx(null);
                  }}
                >
                  Supprimer le bouton
                </button>
              </div>
            </section>
          )}
        </Show>

        <p class="hint security">
          Seules les actions définies ici peuvent être lancées, et seulement par un téléphone associé. Le téléphone ne voit que les libellés et les champs, jamais les programmes ni les
          commandes.
        </p>

        <Show when={dirty() || error()}>
          <div class="save-bar glass">
            <span>{error() ?? "Modifications non enregistrées"}</span>
            <span class="spacer" />
            <button class="btn ghost small" onClick={revert} disabled={busy() || !dirty()}>
              Annuler
            </button>
            <button class="btn small" onClick={save} disabled={busy() || !dirty()}>
              Enregistrer
            </button>
          </div>
        </Show>
      </Show>
    </div>
  );
}
