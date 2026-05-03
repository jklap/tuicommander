import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock @tauri-apps/api/core
vi.mock("@tauri-apps/api/core", () => {
  const mockChannel = class {
    onmessage: ((data: unknown) => void) | null = null;
    id = 1;
  };
  return {
    invoke: vi.fn().mockResolvedValue(undefined),
    Channel: mockChannel,
  };
});

// Mock @tauri-apps/api/event
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

// Mock transport for isTauri
vi.mock("../transport", () => ({
  isTauri: vi.fn().mockReturnValue(true),
  rpc: vi.fn().mockResolvedValue(undefined),
}));

import { createTransport, TauriTransport, WsTransport } from "../components/Terminal/canvasTerminalTransport";
import { isTauri } from "../transport";

describe("canvasTerminalTransport", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe("createTransport", () => {
    it("returns TauriTransport when isTauri() is true", () => {
      (isTauri as ReturnType<typeof vi.fn>).mockReturnValue(true);
      const t = createTransport("session-1");
      expect(t).toBeInstanceOf(TauriTransport);
    });

    it("returns WsTransport when isTauri() is false", () => {
      (isTauri as ReturnType<typeof vi.fn>).mockReturnValue(false);
      const t = createTransport("session-1");
      expect(t).toBeInstanceOf(WsTransport);
    });
  });

  describe("TauriTransport", () => {
    it("subscribes to terminal grid channel via invoke", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      const transport = new TauriTransport("session-1");
      const onFrame = vi.fn();
      await transport.subscribe(onFrame);

      expect(invoke).toHaveBeenCalledWith("subscribe_terminal_grid", expect.objectContaining({
        sessionId: "session-1",
      }));
    });

    it("requests initial frame after subscribe", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      const transport = new TauriTransport("session-1");
      await transport.subscribe(vi.fn());

      expect(invoke).toHaveBeenCalledWith("terminal_request_frame", { sessionId: "session-1" });
    });

    it("delegates invoke calls to Tauri invoke", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      (invoke as ReturnType<typeof vi.fn>).mockResolvedValue("result");
      const transport = new TauriTransport("session-1");
      await transport.subscribe(vi.fn());

      const result = await transport.invoke("terminal_scroll", { sessionId: "session-1", delta: 5 });
      expect(invoke).toHaveBeenCalledWith("terminal_scroll", { sessionId: "session-1", delta: 5 });
      expect(result).toBe("result");
    });

    it("registers event listeners via Tauri listen", async () => {
      const { listen } = await import("@tauri-apps/api/event");
      const transport = new TauriTransport("session-1");
      const handler = vi.fn();
      await transport.subscribe(vi.fn());
      await transport.onEvent("cwd", handler);

      expect(listen).toHaveBeenCalledWith("pty-cwd-session-1", expect.any(Function));
    });

    it("calls unsubscribe_terminal_grid on unsubscribe", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      const transport = new TauriTransport("session-1");
      await transport.subscribe(vi.fn());
      transport.unsubscribe();

      expect(invoke).toHaveBeenCalledWith("unsubscribe_terminal_grid", { sessionId: "session-1" });
    });
  });

  describe("WsTransport", () => {
    it("delegates invoke to rpc()", async () => {
      const { rpc } = await import("../transport");
      (rpc as ReturnType<typeof vi.fn>).mockResolvedValue("ws-result");
      const transport = new WsTransport("session-1");
      const result = await transport.invoke("resize_pty", { sessionId: "session-1", rows: 24, cols: 80 });

      expect(rpc).toHaveBeenCalledWith("resize_pty", { sessionId: "session-1", rows: 24, cols: 80 });
      expect(result).toBe("ws-result");
    });
  });
});
