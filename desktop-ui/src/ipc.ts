import type { AppConfig } from "../../client/src/generated/AppConfig";
import type { Deck } from "../../client/src/generated/Deck";
import type { DeviceView } from "../../client/src/generated/DeviceView";
import type { PairingInfo } from "../../client/src/generated/PairingInfo";
import type { ServerStatus } from "../../client/src/generated/ServerStatus";
import type { UpdateInfo } from "../../client/src/generated/UpdateInfo";

export type { AppConfig, Deck, DeviceView, PairingInfo, ServerStatus, UpdateInfo };

export interface Backend {
  getStatus(): Promise<ServerStatus>;
  setConfig(config: AppConfig): Promise<ServerStatus>;
  setEnabled(enabled: boolean): Promise<ServerStatus>;
  newPairing(): Promise<PairingInfo>;
  listDevices(): Promise<DeviceView[]>;
  revokeDevice(id: string): Promise<boolean>;
  getDeck(): Promise<Deck>;
  /** Enregistre le panneau ; renvoie la version nettoyée par le serveur. */
  setDeck(deck: Deck): Promise<Deck>;
  /** Exécute une action sur ce PC, avec les valeurs par défaut de ses champs. */
  testAction(id: string): Promise<void>;
  hasBackdrop(): Promise<boolean>;
  dataDir(): Promise<string>;
  onStatusChanged(cb: (s: ServerStatus) => void): Promise<() => void>;
  /** Version de l'application et mise à jour trouvée (binaires de la CI seulement). */
  getUpdateInfo(): Promise<UpdateInfo>;
  /** Ouvre la page de la nouvelle version dans le navigateur. */
  openRelease(): Promise<void>;
  onUpdateChanged(cb: (u: UpdateInfo) => void): Promise<() => void>;
}

const inTauri = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function tauriBackend(): Promise<Backend> {
  const { invoke } = await import("@tauri-apps/api/core");
  const { listen } = await import("@tauri-apps/api/event");
  return {
    getStatus: () => invoke("get_status"),
    setConfig: (config) => invoke("set_config", { config }),
    setEnabled: (enabled) => invoke("set_enabled", { enabled }),
    newPairing: () => invoke("new_pairing"),
    listDevices: () => invoke("list_devices"),
    revokeDevice: (id) => invoke("revoke_device", { id }),
    getDeck: () => invoke("get_deck"),
    setDeck: (deck) => invoke("set_deck", { deck }),
    testAction: (id) => invoke("test_action", { id }),
    hasBackdrop: () => invoke("has_backdrop"),
    dataDir: () => invoke("data_dir_path"),
    onStatusChanged: (cb) => listen<ServerStatus>("status-changed", (e) => cb(e.payload)),
    getUpdateInfo: () => invoke("get_update_info"),
    openRelease: () => invoke("open_release"),
    onUpdateChanged: (cb) => listen<UpdateInfo>("update-changed", (e) => cb(e.payload)),
  };
}

export async function loadBackend(): Promise<Backend> {
  if (inTauri()) return tauriBackend();
  const { mockBackend } = await import("./mock");
  return mockBackend();
}
