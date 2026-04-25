import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const { mockListen, mockEmitTo } = vi.hoisted(() => ({
  mockListen: vi.fn(),
  mockEmitTo: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: mockListen,
  emitTo: mockEmitTo,
}));

vi.mock("../../invoke", () => ({
  listen: mockListen,
}));

import {
  createPanelSyncReceiver,
  createPanelSyncProvider,
  type PanelSnapshot,
} from "../../utils/panelSync";

describe("panelSync", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    mockListen.mockReset();
    mockEmitTo.mockReset().mockResolvedValue(undefined);
    // Default: listen captures the callback for manual invocation
    mockListen.mockImplementation(() => Promise.resolve(() => {}));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe("createPanelSyncReceiver", () => {
    it("applies snapshot on matching panel-sync event", () => {
      let capturedCallback: ((event: { payload: PanelSnapshot }) => void) | null = null;
      mockListen.mockImplementation((_event: string, cb: (event: { payload: PanelSnapshot }) => void) => {
        capturedCallback = cb;
        return Promise.resolve(() => {});
      });

      const { state } = createPanelSyncReceiver<{ count: number }>("activity");
      expect(state()).toBeNull();

      capturedCallback!({
        payload: { panelId: "activity", seq: 1, snapshot: { count: 42 } },
      });
      expect(state()).toEqual({ count: 42 });
    });

    it("ignores events for other panels", () => {
      let capturedCallback: ((event: { payload: PanelSnapshot }) => void) | null = null;
      mockListen.mockImplementation((_event: string, cb: (event: { payload: PanelSnapshot }) => void) => {
        capturedCallback = cb;
        return Promise.resolve(() => {});
      });

      const { state } = createPanelSyncReceiver<{ count: number }>("activity");

      capturedCallback!({
        payload: { panelId: "ai-chat", seq: 1, snapshot: { count: 99 } },
      });
      expect(state()).toBeNull();
    });

    it("ignores stale seq numbers", () => {
      let capturedCallback: ((event: { payload: PanelSnapshot }) => void) | null = null;
      mockListen.mockImplementation((_event: string, cb: (event: { payload: PanelSnapshot }) => void) => {
        capturedCallback = cb;
        return Promise.resolve(() => {});
      });

      const { state } = createPanelSyncReceiver<{ count: number }>("activity");

      capturedCallback!({
        payload: { panelId: "activity", seq: 5, snapshot: { count: 50 } },
      });
      expect(state()).toEqual({ count: 50 });

      capturedCallback!({
        payload: { panelId: "activity", seq: 3, snapshot: { count: 30 } },
      });
      expect(state()).toEqual({ count: 50 });

      capturedCallback!({
        payload: { panelId: "activity", seq: 5, snapshot: { count: 55 } },
      });
      expect(state()).toEqual({ count: 50 });
    });

    it("emitAction sends panel-action to main window", async () => {
      mockListen.mockImplementation(() => Promise.resolve(() => {}));
      const { emitAction } = createPanelSyncReceiver("activity");

      await emitAction("navigate", { termId: "t1" });

      expect(mockEmitTo).toHaveBeenCalledWith("main", "panel-action", {
        panelId: "activity",
        action: "navigate",
        data: { termId: "t1" },
      });
    });
  });

  describe("createPanelSyncProvider", () => {
    it("pushes initial snapshot on start", () => {
      const serialize = vi.fn().mockReturnValue({ terminals: [] });
      const provider = createPanelSyncProvider("activity", serialize, 1000);

      provider.start();

      expect(mockEmitTo).toHaveBeenCalledTimes(1);
      expect(mockEmitTo).toHaveBeenCalledWith(
        "panel-activity",
        "panel-sync",
        expect.objectContaining({
          panelId: "activity",
          seq: 1,
          snapshot: { terminals: [] },
        }),
      );

      provider.stop();
    });

    it("pushes snapshots at configured interval", () => {
      const serialize = vi.fn().mockReturnValue({ terminals: [] });
      const provider = createPanelSyncProvider("activity", serialize, 1000);

      provider.start();
      expect(mockEmitTo).toHaveBeenCalledTimes(1);

      vi.advanceTimersByTime(1000);
      expect(mockEmitTo).toHaveBeenCalledTimes(2);

      vi.advanceTimersByTime(1000);
      expect(mockEmitTo).toHaveBeenCalledTimes(3);

      provider.stop();

      vi.advanceTimersByTime(3000);
      expect(mockEmitTo).toHaveBeenCalledTimes(3);
    });

    it("increments seq on each push", () => {
      const serialize = vi.fn().mockReturnValue({});
      const provider = createPanelSyncProvider("activity", serialize, 500);

      provider.start();
      vi.advanceTimersByTime(500);
      vi.advanceTimersByTime(500);

      const calls = mockEmitTo.mock.calls;
      expect(calls[0][2].seq).toBe(1);
      expect(calls[1][2].seq).toBe(2);
      expect(calls[2][2].seq).toBe(3);

      provider.stop();
    });

    it("handles resync requests", () => {
      let resyncCallback: ((event: { payload: { panelId: string } }) => void) | null = null;
      mockListen.mockImplementation((event: string, cb: (event: { payload: { panelId: string } }) => void) => {
        if (event === "panel-resync-request") {
          resyncCallback = cb;
        }
        return Promise.resolve(() => {});
      });

      const serialize = vi.fn().mockReturnValue({ data: "fresh" });
      createPanelSyncProvider("activity", serialize, 5000);

      resyncCallback!({ payload: { panelId: "activity" } });
      expect(mockEmitTo).toHaveBeenCalledWith(
        "panel-activity",
        "panel-sync",
        expect.objectContaining({
          panelId: "activity",
          snapshot: { data: "fresh" },
        }),
      );
    });

    it("ignores resync for other panels", () => {
      let resyncCallback: ((event: { payload: { panelId: string } }) => void) | null = null;
      mockListen.mockImplementation((event: string, cb: (event: { payload: { panelId: string } }) => void) => {
        if (event === "panel-resync-request") {
          resyncCallback = cb;
        }
        return Promise.resolve(() => {});
      });

      const serialize = vi.fn().mockReturnValue({});
      createPanelSyncProvider("activity", serialize, 5000);

      resyncCallback!({ payload: { panelId: "ai-chat" } });
      expect(mockEmitTo).not.toHaveBeenCalled();
    });
  });
});
