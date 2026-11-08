// Faux serveur pour développer et vérifier l'interface dans un simple navigateur.
import type { Backend, AppConfig, DeviceView, ServerStatus } from "./ipc";

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
  let config: AppConfig = { port: 8443, server_enabled: true, address: { kind: "auto" }, launch_at_login: true, start_minimized: false, close_to_tray: true };
  let devices: DeviceView[] = [
    { id: "a", name: "iPhone", paired_at: now() - 86400 * 3, last_seen: now() - 40 },
    { id: "b", name: "iPad", paired_at: now() - 86400 * 9, last_seen: now() - 86400 * 2 },
  ];
  const subs = new Set<(s: ServerStatus) => void>();
  const status = (): ServerStatus => {
    const host = config.address.kind === "mdns" ? "pwamote-yann-pc.local" : config.address.kind === "ip" ? config.address.ip : config.address.kind === "custom" ? config.address.host : "192.168.1.20";
    const params = new URLSearchParams(location.search);
    const error = params.get("error") === "port" && config.server_enabled ? "Le port 8443 est déjà utilisé par un autre programme. Choisissez-en un autre dans les options." : null;
    const running = config.server_enabled && !error;
    return {
      running, port: running ? config.port : null, origin: running ? `https://${host}:${config.port}` : null, host: running ? host : null,
      error, machine: "YANN-PC", connected: running ? 1 : 0, candidates, config,
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
    hasBackdrop: async () => false,
    dataDir: async () => "C:\\Users\\Yann\\AppData\\Roaming\\PwaMote",
    onStatusChanged: async (cb) => (subs.add(cb), () => void subs.delete(cb)),
  };
}
