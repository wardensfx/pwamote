import { render } from "solid-js/web";
import { App } from "./ui/App";
import { RemoteController } from "./lib/remote";
import { pairingCodeFromHash } from "./lib/identity";
import "./styles.css";

const remote = new RemoteController();
const code = pairingCodeFromHash(location.hash);
remote.start(code);

render(() => <App remote={remote} />, document.getElementById("root")!);

// Empêche le zoom par pincement et le double-tap de Safari sur la surface de pointage.
for (const ev of ["gesturestart", "gesturechange"]) document.addEventListener(ev, (e) => e.preventDefault());
