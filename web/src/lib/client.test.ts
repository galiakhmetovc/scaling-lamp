import { describe, expect, it, vi } from "vitest";
import { DaemonClient } from "./client";

class MockWebSocket {
  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSING = 2;
  static readonly CLOSED = 3;

  readonly url: string;
  readyState = MockWebSocket.CONNECTING;
  sent: string[] = [];
  private listeners = new Map<string, Set<(event?: unknown) => void>>();

  constructor(url: string) {
    this.url = url;
  }

  addEventListener(type: string, listener: (event?: unknown) => void) {
    const set = this.listeners.get(type) ?? new Set();
    set.add(listener);
    this.listeners.set(type, set);
  }

  removeEventListener(type: string, listener: (event?: unknown) => void) {
    this.listeners.get(type)?.delete(listener);
  }

  send(payload: string) {
    this.sent.push(payload);
  }

  close() {
    this.readyState = MockWebSocket.CLOSED;
    this.emit("close");
  }

  open() {
    this.readyState = MockWebSocket.OPEN;
    this.emit("open");
  }

  emit(type: string, event?: unknown) {
    for (const listener of this.listeners.get(type) ?? []) {
      listener(event);
    }
  }
}

describe("DaemonClient", () => {
  it("waits for an in-flight websocket connection before sending commands", async () => {
    const sockets: MockWebSocket[] = [];
    const WebSocketMock = vi.fn((url: string) => {
      const socket = new MockWebSocket(url);
      sockets.push(socket);
      return socket;
    });
    Object.assign(WebSocketMock, {
      CONNECTING: MockWebSocket.CONNECTING,
      OPEN: MockWebSocket.OPEN,
      CLOSING: MockWebSocket.CLOSING,
      CLOSED: MockWebSocket.CLOSED,
    });
    vi.stubGlobal("WebSocket", WebSocketMock);
    vi.stubGlobal("window", {
      location: {
        protocol: "http:",
        host: "localhost:8080",
      },
    });

    const client = new DaemonClient({ endpointPath: "/api", websocketPath: "/ws" });
    const pending = client.command("session.get", { session_id: "session-1" });
    await Promise.resolve();

    expect(sockets).toHaveLength(1);
    expect(sockets[0]?.sent).toHaveLength(0);

    sockets[0]?.open();
    await Promise.resolve();
    await Promise.resolve();

    expect(sockets[0]?.sent).toHaveLength(1);
    expect(sockets[0]?.sent[0]).toContain('"command":"session.get"');

    sockets[0]?.emit("message", {
      data: JSON.stringify({
        type: "command_completed",
        id: "web-1",
        command: "session.get",
        payload: { session: { session_id: "session-1" } },
      }),
    });

    await expect(pending).resolves.toEqual({ session: { session_id: "session-1" } });
  });
});
