import { SliderGesture, valueAt } from "../lib/slider-gesture";

/** Moitié de la largeur du bouton du curseur (voir `.range input` dans styles.css). */
const THUMB_HALF = 13;

/**
 * Curseur qui ne vole pas le défilement : la page défile si le doigt part à la verticale,
 * la valeur change seulement s'il part à l'horizontale. L'input natif reste pour l'affichage
 * et le clavier, mais ne reçoit plus les touchers.
 */
export function Range(props: { min: number; max: number; step: number; value: number; onChange: (v: number) => void }) {
  const g = new SliderGesture();
  let input!: HTMLInputElement;

  const apply = (x: number) => {
    const r = input.getBoundingClientRect();
    const v = valueAt(x, { left: r.left, width: r.width, inset: THUMB_HALF }, props.min, props.max, props.step);
    if (v !== props.value) props.onChange(v);
  };

  return (
    <div
      class="range"
      onPointerDown={(e) => g.down(e.clientX, e.clientY)}
      onPointerMove={(e) => {
        const was = g.move(e.clientX, e.clientY);
        if (was !== "drag") return;
        const el = e.currentTarget as HTMLElement;
        if (!el.hasPointerCapture(e.pointerId)) el.setPointerCapture(e.pointerId);
        e.preventDefault();
        apply(e.clientX);
      }}
      onPointerUp={() => g.up()}
      onPointerCancel={() => g.up()}
    >
      <input ref={input} type="range" min={props.min} max={props.max} step={props.step} value={props.value} onInput={(e) => props.onChange(parseFloat(e.currentTarget.value))} />
    </div>
  );
}
