import { batch, createSignal } from "solid-js";
import type { ClientMessage } from "../generated/ClientMessage";
import type { Key } from "../generated/Key";
import type { MagnifierAction } from "../generated/MagnifierAction";
import type { Modifier } from "../generated/Modifier";
import type { MouseButton } from "../generated/MouseButton";
import type { ServerMessage } from "../generated/ServerMessage";
import type { Settings } from "../generated/Settings";
import { Connection, PROTOCOL_VERSION, type ConnectionStatus } from "./connection";
import { encodeMotion, encodeScroll, encodeTrackpad } from "./frames";
import { haptic } from "./haptics";
import { clearIdentity, guessDeviceName, loadIdentity, saveIdentity, wsUrl, type Identity } from "./identity";
import { StickyModifiers } from "./modifiers";
import { MotionSource, requestMotionPermission, saveAxisMap, type MotionPermission } from "./motion";
import { SettingsSync, type Edit } from "./settings-sync";

export type Phase = "needs-pairing" | "connecting" | "ready" | "offline" | "rejected";

/** Un seul contrôleur par application : tout l'état de la télécommande. */
export class RemoteController {
  private identity: Identity | null = loadIdentity();
  private pairingCode: string | null = null;
  private conn: Connection | null = null;
  private motion: MotionSource | null = null;
  private flushTimer: ReturnType<typeof setTimeout> | null = null;
  readonly modifiers = new StickyModifiers(true);

  private phaseSig = createSignal<Phase>(this.identity ? "connecting" : "needs-pairing");
  private settingsSig = createSignal<Settings | null>(null);
  private serverNameSig = createSignal<string>("");
  private magnifierSig = createSignal<{ active: boolean; zoom: number }>({ active: false, zoom: 2.5 });
  private motionSig = createSignal<MotionPermission | "off">("off");
  private modsSig = createSignal<Modifier[]>([]);
  private errorSig = createSignal<string | null>(null);
  private clutchSig = createSignal(false);

  private sync = new SettingsSync((s) => {
    this.settingsSig[1](s);
    this.modifiers.setSticky(s.keyboard.sticky_modifiers);
  });

  readonly phase = () => this.phaseSig[0]();
  readonly settings = () => this.settingsSig[0]();
  readonly serverName = () => this.serverNameSig[0]();
  readonly magnifier = () => this.magnifierSig[0]();
  readonly motionState = () => this.motionSig[0]();
  readonly activeModifiers = () => this.modsSig[0]();
  readonly error = () => this.errorSig[0]();
  readonly clutch = () => this.clutchSig[0]();

  get paired(): boolean {
    return this.identity !== null;
  }

  /** Démarre la connexion : avec le jeton enregistré, ou avec un code d'appairage. */
  start(code: string | null = null): void {
    this.pairingCode = code;
    if (!this.identity && !code) {
      this.phaseSig[1]("needs-pairing");
      return;
    }
    this.conn?.stop();
    this.phaseSig[1]("connecting");
    this.errorSig[1](null);
    this.conn = new Connection({
      url: wsUrl(location),
      handshake: () => this.handshake(),
      onMessage: (m) => this.onServer(m),
      onStatus: (s) => this.onStatus(s),
    });
    this.conn.start();
  }

  private handshake(): ClientMessage | null {
    if (this.pairingCode) {
      return { t: "pair", code: this.pairingCode, device_name: guessDeviceName(), protocol: PROTOCOL_VERSION };
    }
    if (this.identity) return { t: "hello", token: this.identity.token, protocol: PROTOCOL_VERSION };
    return null;
  }

  private onStatus(s: ConnectionStatus): void {
    if (s === "closed") {
      this.sync.onDisconnect();
      this.stopMotion();
      if (this.phase() === "ready" || this.phase() === "connecting") this.phaseSig[1]("offline");
    }
  }

  private onServer(m: ServerMessage): void {
    switch (m.t) {
      case "paired":
        this.identity = { token: m.token, deviceId: m.device_id };
        saveIdentity(this.identity);
        this.pairingCode = null;
        // Le code est à usage unique : on le retire de la barre d'adresse.
        history.replaceState(null, "", location.pathname);
        break;
      case "welcome":
        batch(() => {
          this.serverNameSig[1](m.server_name);
          this.phaseSig[1]("ready");
          this.errorSig[1](null);
        });
        this.pushSettings(this.sync.onServer(m.settings));
        break;
      case "settings":
        this.pushSettings(this.sync.onServer(m.settings));
        break;
      case "magnifier_state":
        this.magnifierSig[1]({ active: m.active, zoom: m.zoom });
        break;
      case "error":
        if (m.code === "unauthorized") {
          // Le PC ne nous connaît plus (appareil révoqué) : il faut ré-appairer.
          this.forget();
        } else if (m.code === "pairing_code_invalid") {
          this.pairingCode = null;
          this.conn?.stop();
          batch(() => {
            this.phaseSig[1]("rejected");
            this.errorSig[1]("Ce QR code a expiré. Affichez-en un nouveau sur le PC.");
          });
        } else if (m.code === "protocol_mismatch") {
          this.conn?.stop();
          batch(() => {
            this.phaseSig[1]("rejected");
            this.errorSig[1]("Mettez à jour l'application ou le serveur : les versions ne correspondent plus.");
          });
        }
        break;
      case "pong":
        break;
    }
  }

  forget(): void {
    this.conn?.stop();
    this.stopMotion();
    clearIdentity();
    this.identity = null;
    batch(() => {
      this.phaseSig[1]("needs-pairing");
      this.settingsSig[1](null);
    });
  }

  private send(m: ClientMessage): void {
    this.conn?.send(m);
  }

  // ----- Boutons, clavier, loupe -----

  button(button: MouseButton, down: boolean): void {
    if (down) this.hapticIf("heavy");
    this.send({ t: "button", button, down });
  }

  tap(key: Key): void {
    this.hapticIf("tap");
    this.send({ t: "key_tap", key, modifiers: this.modifiers.active() });
    this.modifiers.consume();
    this.modsSig[1](this.modifiers.active());
  }

  /** Efface `count` caractères sur le PC (sans modificateurs). */
  backspace(count: number): void {
    for (let i = 0; i < Math.min(count, 200); i++) this.send({ t: "key_tap", key: "backspace", modifiers: [] });
  }

  text(text: string): void {
    const mods = this.modifiers.active();
    if (mods.length > 0 && text.length === 1) {
      // Ctrl+C, Alt+F… : le texte devient un raccourci.
      this.tap(text.toLowerCase() as Key);
      return;
    }
    this.send({ t: "text", text });
  }

  pressModifier(m: Modifier): void {
    this.modifiers.press(m);
    this.hapticIf("tap");
    this.modsSig[1](this.modifiers.active());
  }

  recenter(): void {
    this.hapticIf("tap");
    this.send({ t: "recenter" });
  }

  magnify(action: MagnifierAction): void {
    this.hapticIf("tap");
    this.send({ t: "magnifier", action });
  }

  setClutch(engaged: boolean): void {
    if (this.clutchSig[0]() === engaged) return;
    this.clutchSig[1](engaged);
    this.send({ t: "clutch", engaged });
  }

  trackpad(dx: number, dy: number): void {
    this.conn?.sendBinary(encodeTrackpad(dx, dy));
  }

  scroll(dx: number, dy: number): void {
    this.conn?.sendBinary(encodeScroll(dx, dy));
  }

  // ----- Gyroscope -----

  /** À appeler depuis un geste de l'utilisateur (exigé par iOS). */
  async enableMotion(): Promise<MotionPermission> {
    const permission = await requestMotionPermission();
    this.motionSig[1](permission);
    if (permission === "granted") {
      this.motion ??= new MotionSource((s) => {
        this.conn?.sendBinary(encodeMotion(s));
      });
      this.motion.start();
    }
    return permission;
  }

  /** Oublie les axes mesurés du capteur : ils seront redéterminés au prochain geste. */
  resetGyroAxes(): void {
    this.motion?.resetCalibration();
    saveAxisMap(null);
  }

  private stopMotion(): void {
    this.motion?.stop();
    if (this.motionSig[0]() === "granted") this.motionSig[1]("off");
    this.clutchSig[1](false);
  }

  // ----- Réglages -----

  edit(edit: Edit, key: string | null = null): void {
    this.sync.edit(edit, key);
    if (this.flushTimer) clearTimeout(this.flushTimer);
    // Regroupe les rafales (curseur de sensibilité) en un seul envoi.
    this.flushTimer = setTimeout(() => {
      this.flushTimer = null;
      this.pushSettings(this.sync.flush());
    }, 150);
  }

  private pushSettings(msg: { base_rev: number; settings: Settings } | null): void {
    if (msg) this.send({ t: "update_settings", base_rev: msg.base_rev, settings: msg.settings });
  }

  private hapticIf(kind: "tap" | "heavy"): void {
    if (this.settings()?.feedback.haptics ?? true) haptic(kind);
  }
}
