import { describe, expect, it } from "vitest";
import { Connection, type ConnectionStatus, type SocketLike } from "./connection";

class FakeSocket implements SocketLike {
  binaryType = "blob";
  readyState = 0;
  sent: (string | ArrayBuffer)[] = [];
  onopen: SocketLike["onopen"] = null;
  onclose: SocketLike["onclose"] = null;
  onmessage: SocketLike["onmessage"] = null;
  onerror: SocketLike["onerror"] = null;
  send(d: string | ArrayBuffer) {
    this.sent.push(d);
  }
  close() {
    this.readyState = 3;
  }
  open() {
    this.readyState = 1;
    this.onopen?.({});
  }
  drop() {
    this.readyState = 3;
    this.onclose?.({});
  }
}

function setup(handshake: () => unknown = () => ({ t: "hello", token: "tok", protocol: 1 })) {
  const sockets: FakeSocket[] = [];
  const statuses: ConnectionStatus[] = [];
  const messages: unknown[] = [];
  const timers: { fn: () => void; ms: number }[] = [];
  const conn = new Connection({
    url: "wss://pc.local/ws",
    handshake: handshake as never,
    onMessage: (m) => messages.push(m),
    onStatus: (s) => statuses.push(s),
    createSocket: () => {
      const s = new FakeSocket();
      sockets.push(s);
      return s;
    },
    setTimer: (fn, ms) => {
      timers.push({ fn, ms });
      return timers.length;
    },
    clearTimer: () => {},
  });
  return { conn, sockets, statuses, messages, timers };
}

describe("Connection", () => {
  it("envoie la poignée de main à l'ouverture", () => {
    const { conn, sockets, statuses } = setup();
    conn.start();
    sockets[0]!.open();
    expect(JSON.parse(sockets[0]!.sent[0] as string)).toEqual({ t: "hello", token: "tok", protocol: 1 });
    expect(statuses).toEqual(["connecting", "open"]);
    expect(sockets[0]!.binaryType).toBe("arraybuffer");
  });

  it("se reconnecte avec un délai croissant, puis repart de zéro après un succès", () => {
    const { conn, sockets, timers } = setup();
    conn.start();
    sockets[0]!.drop();
    expect(timers[0]!.ms).toBe(250);
    timers[0]!.fn();
    sockets[1]!.drop();
    expect(timers[1]!.ms).toBe(500);
    timers[1]!.fn();
    sockets[2]!.open();
    sockets[2]!.drop();
    expect(timers[2]!.ms).toBe(250);
  });

  it("ne se reconnecte pas après stop()", () => {
    const { conn, sockets, timers } = setup();
    conn.start();
    sockets[0]!.open();
    conn.stop();
    sockets[0]!.drop();
    expect(timers).toHaveLength(0);
  });

  it("s'arrête si aucune identité n'est disponible", () => {
    const { conn, sockets, timers, statuses } = setup(() => null);
    conn.start();
    sockets[0]!.open();
    expect(sockets[0]!.sent).toHaveLength(0);
    expect(timers).toHaveLength(0);
    expect(statuses.at(-1)).toBe("closed");
  });

  it("ne transmet que les messages valides du serveur", () => {
    const { conn, sockets, messages } = setup();
    conn.start();
    sockets[0]!.open();
    sockets[0]!.onmessage?.({ data: '{"t":"pong","id":1}' });
    sockets[0]!.onmessage?.({ data: "pas du json" });
    sockets[0]!.onmessage?.({ data: new ArrayBuffer(2) });
    expect(messages).toEqual([{ t: "pong", id: 1 }]);
  });

  it("send renvoie false tant que la socket n'est pas ouverte", () => {
    const { conn, sockets } = setup();
    conn.start();
    expect(conn.send({ t: "ping", id: 1 })).toBe(false);
    sockets[0]!.open();
    expect(conn.send({ t: "ping", id: 1 })).toBe(true);
    expect(conn.sendBinary(new ArrayBuffer(1))).toBe(true);
  });
});
