import type { BootstrapPayload, CommandName, CommandPayloadMap, WebsocketEnvelope } from "./types";

type RuntimeClientConfig = {
  endpointPath: string;
  websocketPath: string;
};

type EnvelopeListener = (envelope: WebsocketEnvelope) => void;

type PendingCommand = {
  resolve: (value: unknown) => void;
  reject: (error: Error) => void;
};

export class DaemonClient {
  private readonly endpointPath: string;
  private readonly websocketPath: string;
  private socket: WebSocket | null = null;
  private connectPromise: Promise<void> | null = null;
  private listeners = new Set<EnvelopeListener>();
  private pending = new Map<string, PendingCommand>();
  private nextCommandID = 1;

  constructor(config: RuntimeClientConfig) {
    this.endpointPath = config.endpointPath;
    this.websocketPath = config.websocketPath;
  }

  async bootstrap(signal?: AbortSignal): Promise<BootstrapPayload> {
    const response = await fetch(`${this.endpointPath}/bootstrap`, { signal });
    if (!response.ok) {
      throw new Error(`bootstrap failed with ${response.status}`);
    }
    return response.json() as Promise<BootstrapPayload>;
  }

  async connect(): Promise<void> {
    if (this.socket?.readyState === WebSocket.OPEN) {
      return;
    }
    if (this.connectPromise) {
      return this.connectPromise;
    }
    const scheme = window.location.protocol === "https:" ? "wss" : "ws";
    this.socket = new WebSocket(`${scheme}://${window.location.host}${this.websocketPath}`);
    this.connectPromise = new Promise<void>((resolve, reject) => {
      if (!this.socket) {
        this.connectPromise = null;
        reject(new Error("websocket not created"));
        return;
      }
      const cleanup = () => {
        this.socket?.removeEventListener("open", handleOpen);
        this.socket?.removeEventListener("error", handleError);
      };
      const handleOpen = () => {
        cleanup();
        this.connectPromise = null;
        resolve();
      };
      const handleError = () => {
        cleanup();
        this.connectPromise = null;
        reject(new Error("websocket connection failed"));
      };
      this.socket.addEventListener("open", handleOpen);
      this.socket.addEventListener("error", handleError);
      this.socket.addEventListener("message", (event) => {
        const envelope = JSON.parse(String(event.data)) as WebsocketEnvelope;
        if (envelope.type === "hello") {
          return;
        }
        if (envelope.id && (envelope.type === "command_completed" || envelope.type === "command_failed")) {
          const pending = this.pending.get(envelope.id);
          if (pending) {
            this.pending.delete(envelope.id);
            if (envelope.type === "command_completed") {
              pending.resolve(envelope.payload);
            } else {
              pending.reject(new Error(envelope.error || "daemon command failed"));
            }
          }
        }
        this.listeners.forEach((listener) => listener(envelope));
      });
      this.socket.addEventListener("close", () => {
        this.connectPromise = null;
        const error = new Error("websocket disconnected");
        for (const [id, pending] of this.pending) {
          this.pending.delete(id);
          pending.reject(error);
        }
      });
    });
    await this.connectPromise;
  }

  disconnect(): void {
    this.socket?.close();
    this.socket = null;
    this.connectPromise = null;
  }

  onEnvelope(listener: EnvelopeListener): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  async command<TCommand extends CommandName>(
    command: TCommand,
    payload: Record<string, unknown> = {},
  ): Promise<CommandPayloadMap[TCommand]> {
    await this.connect();
    if (!this.socket || this.socket.readyState !== WebSocket.OPEN) {
      throw new Error("websocket is not connected");
    }
    const id = `web-${this.nextCommandID++}`;
    const result = new Promise<CommandPayloadMap[TCommand]>((resolve, reject) => {
      this.pending.set(id, {
        resolve: (value) => resolve(value as CommandPayloadMap[TCommand]),
        reject,
      });
    });
    this.socket.send(JSON.stringify({ type: "command", id, command, payload }));
    return result;
  }
}

export async function loadRuntimeClientConfig(): Promise<RuntimeClientConfig> {
  const response = await fetch("/config.js");
  if (!response.ok) {
    throw new Error(`client config failed with ${response.status}`);
  }
  const body = await response.text();
  const match = body.match(/window\.__TEAMD_CLIENT_CONFIG__\s*=\s*(\{.*\})\s*;/s);
  if (!match) {
    throw new Error("client config payload is missing");
  }
  const parsed = JSON.parse(match[1]) as { endpointPath?: string; websocketPath?: string };
  return {
    endpointPath: parsed.endpointPath ?? "/api",
    websocketPath: parsed.websocketPath ?? "/ws",
  };
}
