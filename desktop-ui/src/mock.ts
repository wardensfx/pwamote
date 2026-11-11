// Faux serveur pour développer et vérifier l'interface dans un simple navigateur.
import type { Backend, AppConfig, Deck, DeviceView, ServerStatus } from "./ipc";

const keys = (id: string, label: string, icon: string, key: string, modifiers: string[] = [], color = "glass") =>
  ({ id, label, icon, color, inputs: [], steps: [{ kind: "keys", key, modifiers }] }) as never;

/** Même exemple que le premier lancement du serveur, plus une action qui lance un script. */
const exampleDeck = (): Deck => ({
  pages: [
    {
      id: "video",
      name: "Vidéo",
      columns: 3,
      rows: 2,
      cells: [
        keys("slower", "Ralentir", "🐢", "s", [], "blue"),
        keys("faster", "Accélérer", "🐇", "d", [], "amber"),
        keys("key_q", "Touche Q", "Q", "q"),
        keys("play", "Lecture / pause", "⏯", "media_play_pause"),
        keys("fullscreen", "Plein écran", "⛶", "f"),
        {
          id: "speed",
          label: "Vitesse précise",
          icon: "⏱",
          color: "violet",
          inputs: [{ id: "vitesse", label: "Vitesse", kind: { type: "slider", min: 0.25, max: 4, step: 0.25, default: 1, unit: "×" } }],
          steps: [{ kind: "run", program: "powershell.exe", args: ["-NoProfile", "-File", "C:\\Scripts\\vitesse.ps1", "{vitesse}"], cwd: "", hidden: true }],
        },
      ],
    },
    {
      id: "shortcuts",
      name: "Raccourcis",
      columns: 3,
      rows: 2,
      cells: [
        keys("copy", "Copier", "⧉", "c", ["ctrl"]),
        keys("paste", "Coller", "📋", "v", ["ctrl"]),
        keys("undo", "Annuler", "↶", "z", ["ctrl"]),
        keys("desktop", "Bureau", "🖥", "d", ["meta"]),
        keys("capture", "Capture", "✂", "s", ["meta", "shift"]),
        {
          id: "youtube",
          label: "Chercher sur YouTube",
          icon: "🔎",
          color: "red",
          inputs: [{ id: "recherche", label: "Recherche", kind: { type: "text", default: "", placeholder: "Titre, chaîne…" } }],
          steps: [{ kind: "open", target: "https://www.youtube.com/results?search_query={recherche:url}" }],
        },
      ],
    },
  ],
});

function fakeQr(seed: string): string {
  const n = 29;
  let h = 2166136261;
  for (const c of seed) h = Math.imul(h ^ c.charCodeAt(0), 16777619);
  const rnd = () => ((h = Math.imul(h ^ (h >>> 15), 2246822519) >>> 0) / 4294967296);
  const finder = (x: number, y: number) => {
    const inF = (ox: number, oy: number) => x >= ox && x < ox + 7 && y >= oy && y < oy + 7;
    for (const [ox, oy] of [[0, 0], [n - 7, 0], [0, n - 7]] as const) {
      if (inF(ox!, oy!)) {
        const dx = Math.min(x - ox!, ox! + 6 - x), dy = Math.min(y - oy!, oy! + 6 - y), d = Math.min(dx, dy);
        return d === 0 || d >= 2;
      }
    }
    return null;
  };
  let rects = "";
  for (let y = 0; y < n; y++) for (let x = 0; x < n; x++) {
    const f = finder(x, y);
    if (f === true || (f === null && rnd() > 0.52)) rects += `<rect x="${x + 2}" y="${y + 2}" width="1" height="1"/>`;
  }
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${n + 4} ${n + 4}" shape-rendering="crispEdges"><rect width="100%" height="100%" fill="#fff"/><g fill="#0b0c0f">${rects}</g></svg>`;
}

export function mockBackend(): Backend {
  const now = () => Math.floor(Date.now() / 1000);
  const candidates = [
    { choice: { kind: "auto" } as const, host: "192.168.1.20", label: "192.168.1.20", recommended: true },
    { choice: { kind: "ip", ip: "10.8.0.4" } as const, host: "10.8.0.4", label: "10.8.0.4", recommended: false },
    { choice: { kind: "mdns" } as const, host: "pwamote-yann-pc.local", label: "pwamote-yann-pc.local", recommended: false },
  ];
  let config: AppConfig = { port: 8443, server_enabled: true, address: { kind: "auto" }, launch_at_login: true, start_minimized: false, close_to_tray: true,
    public_cert: { mode: "auto", domain: "", email: "", provider: { kind: "duck_dns", token: "" }, staging: false } };
  let devices: DeviceView[] = [
    { id: "a", name: "iPhone", paired_at: now() - 86400 * 3, last_seen: now() - 40 },
    { id: "b", name: "iPad", paired_at: now() - 86400 * 9, last_seen: now() - 86400 * 2 },
  ];
  let deck = exampleDeck();
  const subs = new Set<(s: ServerStatus) => void>();
  const status = (): ServerStatus => {
    const local = config.address.kind === "mdns" ? "pwamote-yann-pc.local" : config.address.kind === "ip" ? config.address.ip : config.address.kind === "custom" ? config.address.host : "192.168.1.20";
    const pc = config.public_cert;
    const token = pc.provider.kind === "duck_dns" ? pc.provider.token : pc.provider.api_token;
    const setup = new URLSearchParams(location.search).get("setup") === "1";
    const ready = (pc.mode === "auto" && !setup) || (pc.mode === "custom" && pc.domain.includes(".") && token !== "");
    const name = pc.mode === "auto" ? "k3x9f2ab7qzm.d.pwamote.app" : pc.domain;
    const now = Math.floor(Date.now() / 1000);
    const public_cert: ServerStatus["public_cert"] = {
      phase: pc.mode === "off" ? "disabled" : setup ? "obtaining" : ready ? "active" : "incomplete",
      domain: name || null,
      message: setup ? "Demande du certificat à Let's Encrypt : validation DNS, une à deux minutes…" : pc.mode !== "off" && !ready ? "Indiquez le nom de domaine et le jeton." : null,
      expires_at: ready ? now + 74 * 86400 : null,
      next_check_at: now + 6 * 3600,
      online: true,
      staging: pc.staging,
    };
    const host = ready ? name : local;
    const params = new URLSearchParams(location.search);
    const error = params.get("error") === "port" && config.server_enabled ? "Le port 8443 est déjà utilisé par un autre programme. Choisissez-en un autre dans les options." : null;
    const running = config.server_enabled && !error;
    return {
      running, port: running ? config.port : null, origin: running ? `https://${host}:${config.port}` : null, host: running ? host : null,
      error, machine: "YANN-PC", connected: running ? 1 : 0, candidates, config, public_cert,
    };
  };
  const push = () => subs.forEach((cb) => cb(status()));
  return {
    getStatus: async () => status(),
    setConfig: async (c) => ((config = c), push(), status()),
    setEnabled: async (e) => ((config = { ...config, server_enabled: e }), push(), status()),
    newPairing: async () => {
      const s = status();
      if (!s.running) throw new Error("le serveur est arrêté");
      const url = `${s.origin}/#/pair/${Math.random().toString(36).slice(2, 12)}Xk3pQ9`;
      return { url, svg: fakeQr(url), host: s.host! };
    },
    listDevices: async () => devices,
    revokeDevice: async (id) => ((devices = devices.filter((d) => d.id !== id)), true),
    getDeck: async () => JSON.parse(JSON.stringify(deck)),
    setDeck: async (d) => ((deck = JSON.parse(JSON.stringify(d))), JSON.parse(JSON.stringify(deck))),
    testAction: async (id) => {
      await new Promise((r) => setTimeout(r, 300));
      const b = deck.pages.flatMap((p) => p.cells).find((c) => c?.id === id);
      if (b?.steps.some((s) => s.kind === "run")) throw new Error("programme introuvable : powershell.exe (simulation)");
    },
    hasBackdrop: async () => false,
    dataDir: async () => "C:\\Users\\Yann\\AppData\\Roaming\\PwaMote",
    onStatusChanged: async (cb) => (subs.add(cb), () => void subs.delete(cb)),
  };
}
