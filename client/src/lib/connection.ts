import type { ClientMessage } from "../generated/ClientMessage";
import type { ServerMessage } from "../generated/ServerMessage";

export const PROTOCOL_VERSION = 1;

export type ConnectionStatus = "idle" | "connecting" | "open" | "closed";

export interface SocketLike {
  binaryType: string;
  readyState: number;
  send(data: string | ArrayBuffer): void;
  close(): void;
  onopen: ((ev: unknown) => void) | null;
  onclose: ((ev: unknown) => void) | null;
  onmessage: ((ev: { data: unknown }) => void) | null;
  onerror: ((ev: unknown) => void) | null;
}

export interface ConnectionOptions {
  url: string;
  /** Message d'ouverture de session : `hello` (jeton) ou `pair` (code). */
  handshake: () => ClientMessage | null;
  onMessage: (msg: ServerMessage) => void;
  onStatus: (status: ConnectionStatus) => void;
  createSocket?: (url: string) => SocketLike;
  setTimer?: (fn: () => void, ms: number) => unknown;
  clearTimer?: (id: unknown) => void;
  /** Délais de reconnexion successifs, en ms. Le dernier se répète. */
  backoff?: number[];
}

const OPEN = 1;

/** WebSocket avec reconnexion automatique et poignée de main à chaque ouverture. */
export class Connection {
  private socket: SocketLike | null = null;
  private attempt = 0;
  private timer: unknown = null;
  private stopped = true;
  private readonly backoff: number[];

  constructor(private readonly opts: ConnectionOptions) {
    this.backoff = opts.backoff ?? [250, 500, 1000, 2000, 4000];
  }

  start(): void {
    this.stopped = false;
    this.connect();
  }

  stop(): void {
    this.stopped = true;
    if (this.timer !== null) (this.opts.clearTimer ?? clearTimeout)(this.timer as never);
    this.timer = null;
    const s = this.socket;
    this.socket = null;
    if (s) {
      s.onclose = null;
      s.close();
    }
    this.opts.onStatus("closed");
  }

  get isOpen(): boolean {
    return this.socket?.readyState === OPEN;
  }

  send(msg: ClientMessage): boolean {
    if (!this.isOpen) return false;
    this.socket!.send(JSON.stringify(msg));
    return true;
  }

  sendBinary(buf: ArrayBuffer): boolean {
    if (!this.isOpen) return false;
    this.socket!.send(buf);
    return true;
  }

  private connect(): void {
    this.opts.onStatus("connecting");
    const create = this.opts.createSocket ?? ((u: string) => new WebSocket(u) as unknown as SocketLike);
    const socket = create(this.opts.url);
    socket.binaryType = "arraybuffer";
    this.socket = socket;

    socket.onopen = () => {
      const hello = this.opts.handshake();
      if (!hello) {
        // Rien pour s'authentifier : inutile d'insister.
        this.stop();
        return;
      }
      this.attempt = 0;
      socket.send(JSON.stringify(hello));
      this.opts.onStatus("open");
    };
    socket.onmessage = (ev) => {
      if (typeof ev.data !== "string") return;
      try {
        this.opts.onMessage(JSON.parse(ev.data) as ServerMessage);
      } catch {
        /* message illisible : ignoré */
      }
    };
    socket.onclose = () => {
      if (this.socket !== socket) return;
      this.socket = null;
      this.opts.onStatus("closed");
      if (!this.stopped) this.scheduleReconnect();
    };
    socket.onerror = () => {
      /* `onclose` suit toujours */
    };
  }

  private scheduleReconnect(): void {
    const delay = this.backoff[Math.min(this.attempt, this.backoff.length - 1)] ?? 1000;
    this.attempt += 1;
    this.timer = (this.opts.setTimer ?? setTimeout)(() => {
      this.timer = null;
      if (!this.stopped) this.connect();
    }, delay);
  }
}
