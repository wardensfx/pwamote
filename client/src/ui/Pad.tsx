import { Show, createMemo, createSignal, onCleanup } from "solid-js";
import { PadGestures } from "../lib/pad-gestures";
import type { RemoteController } from "../lib/remote";

export function Pad(props: { remote: RemoteController; onTouch?: () => void }) {
  const r = props.remote;
  const mode = () => r.settings()?.pointer.mode ?? "air_hold";
  const gestures = new PadGestures(
    {
      clutch: (e) => r.setClutch(e),
      trackpad: (dx, dy) => r.trackpad(dx, dy),
      scroll: (dx, dy) => r.scroll(dx, dy),
      click: () => {
        r.button("left", true);
        r.button("left", false);
        flash();
      },
      press: (down) => {
        r.button("left", down);
        setDown(down);
      },
    },
    mode,
  );
  // En gyro continu, la surface est le bouton gauche : elle s'enfonce visiblement.
  const [down, setDown] = createSignal(false);
  let flashTimer: ReturnType<typeof setTimeout> | undefined;
  const flash = () => {
    setDown(true);
    clearTimeout(flashTimer);
    flashTimer = setTimeout(() => setDown(false), 140);
  };
  onCleanup(() => {
    gestures.cancel();
    clearTimeout(flashTimer);
  });

  const needsMotion = createMemo(() => mode() !== "trackpad" && r.motionState() !== "granted");

  const hint = createMemo(() => {
    switch (mode()) {
      case "air_always":
        return ["Clic gauche", "Maintenez pour glisser-déposer · balayez pour défiler"];
      case "air_hold":
        return ["Posez le pouce et visez", "Levez-le pour lâcher le curseur · glissez-le pour défiler"];
      default:
        return ["Glissez pour déplacer", "Tapez pour cliquer · deux doigts pour défiler"];
    }
  });

  return (
    <div
      class="glass pad"
      data-mode={mode()}
      data-engaged={r.clutch()}
      data-down={down()}
      onPointerDown={(e) => {
        if ((e.target as HTMLElement).closest("button")) return;
        (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
        props.onTouch?.();
        gestures.down(e.pointerId, e.clientX, e.clientY, e.timeStamp);
      }}
      onPointerMove={(e) => gestures.move(e.pointerId, e.clientX, e.clientY, e.timeStamp)}
      onPointerUp={(e) => gestures.up(e.pointerId, e.timeStamp)}
      onPointerCancel={() => gestures.cancel()}
      onContextMenu={(e) => e.preventDefault()}
    >
      <Show when={mode() !== "air_always"}>
        <div class="reticle" />
      </Show>
      <Show when={needsMotion()}>
        <button class="btn small enable" onClick={() => void r.enableMotion()}>
          {r.motionState() === "denied" ? "Autoriser dans Réglages › Safari" : "Activer le gyroscope"}
        </button>
      </Show>
      <div class="hint">
        {hint()[0]}
        <small>{hint()[1]}</small>
      </div>
    </div>
  );
}
