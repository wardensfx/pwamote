import jsQR from "jsqr";
import { Show, createSignal, onCleanup, onMount } from "solid-js";

/** Ouvre la caméra arrière et appelle `onText` avec le contenu du premier QR code lu. */
export function QrScanner(props: { onText: (text: string) => void; onClose: () => void }) {
  let video!: HTMLVideoElement;
  const [error, setError] = createSignal<string | null>(null);
  let stream: MediaStream | null = null;
  let raf = 0;
  let done = false;

  const canvas = document.createElement("canvas");
  const ctx = canvas.getContext("2d", { willReadFrequently: true });

  const scan = () => {
    if (done) return;
    if (ctx && video.readyState >= video.HAVE_CURRENT_DATA && video.videoWidth > 0) {
      const scale = Math.min(1, 640 / video.videoWidth);
      canvas.width = Math.round(video.videoWidth * scale);
      canvas.height = Math.round(video.videoHeight * scale);
      ctx.drawImage(video, 0, 0, canvas.width, canvas.height);
      const img = ctx.getImageData(0, 0, canvas.width, canvas.height);
      const found = jsQR(img.data, img.width, img.height, { inversionAttempts: "dontInvert" });
      if (found?.data) {
        done = true;
        props.onText(found.data);
        return;
      }
    }
    raf = requestAnimationFrame(scan);
  };

  onMount(async () => {
    try {
      stream = await navigator.mediaDevices.getUserMedia({ video: { facingMode: { ideal: "environment" } }, audio: false });
      video.srcObject = stream;
      await video.play();
      raf = requestAnimationFrame(scan);
    } catch {
      setError("Impossible d'accéder à la caméra. Autorisez-la dans Réglages › Safari, ou scannez le QR code avec l'appareil photo du téléphone.");
    }
  });

  onCleanup(() => {
    done = true;
    cancelAnimationFrame(raf);
    stream?.getTracks().forEach((t) => t.stop());
  });

  return (
    <div class="scanner">
      <video ref={video} playsinline muted />
      <div class="frame" />
      <Show when={error()} fallback={<p class="scan-hint">Placez le QR code affiché sur le PC dans le cadre.</p>}>
        <p class="scan-hint">{error()}</p>
      </Show>
      <button class="btn ghost scan-close" onClick={props.onClose}>
        Annuler
      </button>
    </div>
  );
}
