import { isTauri, rpc } from "../../transport";

export interface TerminalTransport {
  subscribe(onFrame: (data: ArrayBuffer) => void): Promise<void>;
  unsubscribe(): void;
  invoke(cmd: string, args: Record<string, unknown>): Promise<unknown>;
  onEvent(type: string, handler: (payload: unknown) => void): Promise<void>;
}

export function createTransport(sessionId: string): TerminalTransport {
  return isTauri() ? new TauriTransport(sessionId) : new WsTransport(sessionId);
}

export class TauriTransport implements TerminalTransport {
  private sessionId: string;
  private invokeRef: ((cmd: string, args?: Record<string, unknown>) => Promise<unknown>) | null = null;
  private unlisteners: (() => void)[] = [];

  constructor(sessionId: string) {
    this.sessionId = sessionId;
  }

  async subscribe(onFrame: (data: ArrayBuffer) => void): Promise<void> {
    const { invoke, Channel } = await import("@tauri-apps/api/core");
    this.invokeRef = invoke;
    const channel = new Channel<ArrayBuffer | number[]>();
    channel.onmessage = (data) => {
      if (data instanceof ArrayBuffer) {
        onFrame(data);
      } else {
        onFrame(new Uint8Array(data).buffer);
      }
    };
    await invoke("subscribe_terminal_grid", {
      sessionId: this.sessionId,
      channel,
    });
    invoke("terminal_request_frame", { sessionId: this.sessionId }).catch(() => {});
  }

  unsubscribe(): void {
    this.invokeRef?.("unsubscribe_terminal_grid", { sessionId: this.sessionId }).catch(() => {});
    for (const unlisten of this.unlisteners) unlisten();
    this.unlisteners = [];
  }

  async invoke(cmd: string, args: Record<string, unknown>): Promise<unknown> {
    if (!this.invokeRef) {
      const { invoke } = await import("@tauri-apps/api/core");
      this.invokeRef = invoke;
    }
    return this.invokeRef(cmd, args);
  }

  async onEvent(type: string, handler: (payload: unknown) => void): Promise<void> {
    const { listen } = await import("@tauri-apps/api/event");
    const eventName = `pty-${type}-${this.sessionId}`;
    const unlisten = await listen(eventName, (event: { payload: unknown }) => {
      handler(event.payload);
    });
    this.unlisteners.push(unlisten);
  }
}

export class WsTransport implements TerminalTransport {
  private sessionId: string;
  private ws: WebSocket | null = null;
  private onFrameHandler: ((data: ArrayBuffer) => void) | null = null;
  private eventHandlers = new Map<string, (payload: unknown) => void>();

  constructor(sessionId: string) {
    this.sessionId = sessionId;
  }

  async subscribe(onFrame: (data: ArrayBuffer) => void): Promise<void> {
    this.onFrameHandler = onFrame;
    const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
    const url = `${proto}//${window.location.host}/sessions/${this.sessionId}/stream?format=grid`;
    this.ws = new WebSocket(url);
    this.ws.binaryType = "arraybuffer";
    this.ws.onmessage = (e) => {
      if (e.data instanceof ArrayBuffer) {
        this.onFrameHandler?.(e.data);
      } else {
        try {
          const event = JSON.parse(e.data as string) as { type: string; payload: unknown };
          this.eventHandlers.get(event.type)?.(event.payload);
        } catch {
          // ignore non-JSON text messages
        }
      }
    };
    await new Promise<void>((resolve, reject) => {
      this.ws!.onopen = () => resolve();
      this.ws!.onerror = () => reject(new Error("WebSocket connection failed"));
    });
  }

  unsubscribe(): void {
    this.ws?.close();
    this.ws = null;
    this.eventHandlers.clear();
  }

  async invoke(cmd: string, args: Record<string, unknown>): Promise<unknown> {
    return rpc(cmd, args);
  }

  async onEvent(type: string, handler: (payload: unknown) => void): Promise<void> {
    this.eventHandlers.set(type, handler);
  }
}
