import { render } from "solid-js/web";
import { App } from "./ui/App";
import { RemoteController } from "./lib/remote";
import { pairingCodeFromHash } from "./lib/identity";
import { registerSW } from "virtual:pwa-register";
import "./styles.css";

const remote = new RemoteController();
const code = pairingCodeFromHash(location.hash);
remote.start(code);

render(() => <App remote={remote} />, document.getElementById("root")!);

// Empêche le zoom par pincement et le double-tap de Safari sur la surface de pointage.
for (const ev of ["gesturestart", "gesturechange"]) document.addEventListener(ev, (e) => e.preventDefault());

// Une PWA installée reste en mémoire des jours sans être rechargée : on cherche une nouvelle version
// à chaque retour au premier plan et toutes les minutes. Dès qu'elle est prête, la page se recharge
// (mode autoUpdate du plugin).
registerSW({
  immediate: true,
  onRegisteredSW(_url, registration) {
    if (!registration) return;
    const check = () => void registration.update().catch(() => {});
    setInterval(check, 60_000);
    document.addEventListener("visibilitychange", () => document.visibilityState === "visible" && check());
    window.addEventListener("online", check);
  },
});
