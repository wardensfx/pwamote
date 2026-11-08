import { render } from "solid-js/web";
import { App } from "./App";
import { loadBackend } from "./ipc";
import "./styles.css";

loadBackend().then((backend) => render(() => <App backend={backend} />, document.getElementById("root")!));
// Pas de menu contextuel du navigateur dans une application de bureau.
document.addEventListener("contextmenu", (e) => e.preventDefault());
