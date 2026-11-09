import { batch, createSignal } from "solid-js";
import type { ClientMessage } from "../generated/ClientMessage";
import type { DeckView } from "../generated/DeckView";
import type { Key } from "../generated/Key";
import type { MagnifierAction } from "../generated/MagnifierAction";
import type { Modifier } from "../generated/Modifier";
import type { MouseButton } from "../generated/MouseButton";
import type { ServerMessage } from "../generated/ServerMessage";
import type { Settings } from "../generated/Settings";
import { readPhoneClipboard, sendClip, shouldOfferOnReturn, type PhoneClip } from "./clipboard";
import { Connection, PROTOCOL_VERSION, type ConnectionStatus } from "./connection";
import { encodeLinear, encodeMotion, encodeScroll, encodeTrackpad } from "./frames";
import { haptic } from "./haptics";
import { clearIdentity, guessDeviceName, loadIdentity, saveIdentity, wsUrl, type Identity } from "./identity";
import { StickyModifiers } from "./modifiers";
import { MotionSource, requestMotionPermission, saveAxisMap, type MotionPermission } from "./motion";
import { OrientationSource } from "./orientation";
import { SettingsSync, type Edit } from "./settings-sync";
import type { Values } from "./actions";

/** État visible d'un bouton d'action juste après un appui. */
export type ActionState = "running" | "ok" | "error";

/** Bandeau d'état d'un envoi du presse-papiers vers le PC. */
export type ClipStatus = { state: "sending" | "ok" | "error"; message: string };

export type Phase = "needs-pairing" | "connecting" | "ready" | "offline" | "rejected";

/** Un seul contrôleur par application : tout l'état de la télécommande. */
export class RemoteController {
  private identity: Identity | null = loadIdentity();
  private pairingCode: string | null = null;
  private conn: Connection | null = null;
  private motion: MotionSource | null = null;
  private orientation: OrientationSource | null = null;
  private fallbackTimer: ReturnType<typeof setTimeout> | null = null;
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
  private deckSig = createSignal<DeckView | null>(null);
  private actionStatesSig = createSignal<Record<string, ActionState>>({});
  private actionErrorSig = createSignal<string | null>(null);
  private actionTimers = new Map<string, ReturnType<typeof setTimeout>>();
  private actionErrorTimer: ReturnType<typeof setTimeout> | null = null;
  private clipSig = createSignal<ClipStatus | null>(null);
  private clipOfferSig = createSignal(false);
  private clipTimer: ReturnType<typeof setTimeout> | null = null;
  private clipOfferTimer: ReturnType<typeof setTimeout> | null = null;
  private hiddenAt = 0;

  private sync = new SettingsSync((s) => {
    this.settingsSig[1](s);
    this.modifiers.setSticky(s.keyboard.sticky_modifiers);
    this.orientation?.setLinear(s.pointer.translation && this.orientation.received > 0);
  });

  readonly phase = () => this.phaseSig[0]();
  readonly settings = () => this.settingsSig[0]();
  readonly serverName = () => this.serverNameSig[0]();
  readonly magnifier = () => this.magnifierSig[0]();
  readonly motionState = () => this.motionSig[0]();
  readonly activeModifiers = () => this.modsSig[0]();
  readonly error = () => this.errorSig[0]();
  readonly clutch = () => this.clutchSig[0]();
  readonly deck = () => this.deckSig[0]();
  readonly actionStates = () => this.actionStatesSig[0]();
  readonly actionError = () => this.actionErrorSig[0]();
  readonly clipStatus = () => this.clipSig[0]();
  /** Proposer d'envoyer le presse-papiers (on revient dans l'app, sans doute après avoir copié). */
  readonly clipOffer = () => this.clipOfferSig[0]();

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
      case "deck":
        this.deckSig[1](m.deck);
        break;
      case "action_result":
        this.setActionState(m.id, m.ok ? "ok" : "error", 900);
        if (!m.ok) this.showActionError(m.message ?? "L'action a échoué sur le PC.");
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
      this.deckSig[1](null);
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

  // ----- Actions programmables -----

  /** Lance un bouton du panneau. Le PC vérifie qu'il existe et valide chaque valeur. */
  runAction(id: string, values: Values = {}): void {
    this.hapticIf("tap");
    // Sans réponse (ancien serveur, réseau coupé), le bouton revient au repos.
    this.setActionState(id, "running", 8000);
    this.send({ t: "run_action", id, values });
  }

  private setActionState(id: string, state: ActionState, clearAfter: number): void {
    const prev = this.actionTimers.get(id);
    if (prev) clearTimeout(prev);
    this.actionStatesSig[1]((s) => ({ ...s, [id]: state }));
    this.actionTimers.set(
      id,
      setTimeout(() => {
        this.actionTimers.delete(id);
        this.actionStatesSig[1](({ [id]: _, ...rest }) => rest);
      }, clearAfter),
    );
  }

  private showActionError(message: string): void {
    if (this.actionErrorTimer) clearTimeout(this.actionErrorTimer);
    this.actionErrorSig[1](message);
    this.actionErrorTimer = setTimeout(() => this.actionErrorSig[1](null), 4000);
  }

  // ----- Presse-papiers vers le PC -----

  /**
   * Lit le presse-papiers du téléphone et l'envoie au PC. À appeler directement depuis un tap :
   * la lecture démarre avant tout `await`, comme l'exige iOS.
   */
  sendClipboard(): Promise<void> {
    return this.deliverClip(readPhoneClipboard());
  }

  /** Envoie un contenu déjà obtenu (image collée dans un champ). */
  sendClip(clip: PhoneClip): Promise<void> {
    return this.deliverClip(Promise.resolve(clip));
  }

  /** À brancher sur `visibilitychange` : au retour dans l'app, propose l'envoi quelques secondes. */
  onVisibility(hidden: boolean, now: number = Date.now()): void {
    if (hidden) {
      this.hiddenAt = now;
      this.clipOfferSig[1](false);
      return;
    }
    const enabled = (this.settings()?.clipboard.to_pc ?? false) && this.phase() === "ready";
    if (!shouldOfferOnReturn(enabled, this.hiddenAt, now)) return;
    this.clipOfferSig[1](true);
    if (this.clipOfferTimer) clearTimeout(this.clipOfferTimer);
    this.clipOfferTimer = setTimeout(() => this.clipOfferSig[1](false), 8000);
  }

  dismissClipOffer(): void {
    this.clipOfferSig[1](false);
  }

  private async deliverClip(reading: Promise<PhoneClip | null>): Promise<void> {
    this.clipOfferSig[1](false);
    const token = this.identity?.token;
    if (!token) return;
    this.clipSig[1]({ state: "sending", message: "Envoi au PC…" });
    this.setClipTimer(30_000, () => this.clipSig[1](null));
    try {
      const clip = await reading;
      if (!clip) throw new Error("Le presse-papiers du téléphone est vide.");
      await sendClip(clip, token);
      this.hapticIf("tap");
      const what = clip.type.startsWith("image/") ? "Image copiée" : "Texte copié";
      this.clipSig[1]({ state: "ok", message: `${what} sur le PC · Ctrl+V` });
      this.setClipTimer(2200, () => this.clipSig[1](null));
    } catch (e) {
      this.clipSig[1]({ state: "error", message: e instanceof Error ? e.message : String(e) });
      this.setClipTimer(4500, () => this.clipSig[1](null));
    }
  }

  private setClipTimer(ms: number, fn: () => void): void {
    if (this.clipTimer) clearTimeout(this.clipTimer);
    this.clipTimer = setTimeout(() => {
      this.clipTimer = null;
      fn();
    }, ms);
  }

  // ----- Gyroscope -----

  /** À appeler depuis un geste de l'utilisateur (exigé par iOS). */
  async enableMotion(): Promise<MotionPermission> {
    const permission = await requestMotionPermission();
    this.motionSig[1](permission);
    if (permission === "granted") {
      const send = (s: Parameters<typeof encodeMotion>[0]) => this.conn?.sendBinary(encodeMotion(s));
      // Méthode privilégiée : angles d'orientation, aux axes définis sans ambiguïté.
      this.orientation ??= new OrientationSource(send, {
        onLinear: (accel, dt) => this.conn?.sendBinary(encodeLinear(accel, dt)),
      });
      this.motion ??= new MotionSource(send);
      this.orientation.start();
      // Navigateur sans `deviceorientation` : repli sur le gyroscope brut avec calibration des axes.
      if (this.fallbackTimer) clearTimeout(this.fallbackTimer);
      this.fallbackTimer = setTimeout(() => {
        this.fallbackTimer = null;
        if (this.orientation && this.orientation.received === 0) {
          this.orientation.stop();
          this.motion?.start();
        } else {
          this.orientation?.setLinear(this.settings()?.pointer.translation ?? false);
        }
      }, 1500);
    }
    return permission;
  }

  /** Oublie les axes mesurés du capteur : ils seront redéterminés au prochain geste. */
  resetGyroAxes(): void {
    this.motion?.resetCalibration();
    saveAxisMap(null);
  }

  private stopMotion(): void {
    if (this.fallbackTimer) clearTimeout(this.fallbackTimer);
    this.fallbackTimer = null;
    this.orientation?.stop();
    this.motion?.stop();
    if (this.orientation) this.orientation.received = 0;
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
