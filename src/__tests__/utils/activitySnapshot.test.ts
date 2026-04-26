import { describe, it, expect, vi, beforeEach } from "vitest";

const mockInvoke = vi.fn().mockResolvedValue(undefined);

vi.mock("@tauri-apps/api/core", () => ({
  invoke: mockInvoke,
}));

describe("activitySnapshot", () => {
  let buildActivitySnapshot: typeof import("../../utils/activitySnapshot").buildActivitySnapshot;
  let terminalsStore: typeof import("../../stores/terminals").terminalsStore;
  let globalWorkspaceStore: typeof import("../../stores/globalWorkspace").globalWorkspaceStore;

  beforeEach(async () => {
    vi.resetModules();
    mockInvoke.mockReset().mockResolvedValue(undefined);
    vi.doMock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));

    const termMod = await import("../../stores/terminals");
    terminalsStore = termMod.terminalsStore;
    const gwMod = await import("../../stores/globalWorkspace");
    globalWorkspaceStore = gwMod.globalWorkspaceStore;
    const snapMod = await import("../../utils/activitySnapshot");
    buildActivitySnapshot = snapMod.buildActivitySnapshot;
  });

  it("returns empty terminals array when none exist", () => {
    const snap = buildActivitySnapshot();
    expect(snap.terminals).toEqual([]);
  });

  it("includes all terminal fields in snapshot", () => {
    const id = terminalsStore.add({
      name: "Terminal 1",
      sessionId: "sess1",
      cwd: "/Users/test/project",
      fontSize: 14,
      awaitingInput: null,
      agentType: "claude",
    });
    terminalsStore.update(id, {
      shellState: "busy",
      agentIntent: "Writing tests",
      lastPrompt: "Write tests for panelSync",
    });

    const snap = buildActivitySnapshot();
    expect(snap.terminals).toHaveLength(1);

    const t = snap.terminals[0];
    expect(t.id).toBe(id);
    expect(t.name).toBe("Terminal 1");
    expect(t.shellState).toBe("busy");
    expect(t.awaitingInput).toBeNull();
    expect(t.sessionId).toBe("sess1");
    expect(t.agentType).toBe("claude");
    expect(t.agentIntent).toBe("Writing tests");
    expect(t.currentTask).toBeNull(); // claude agentType suppresses currentTask
    expect(t.lastPrompt).toBe("Write tests for panelSync");
    expect(t.cwd).toBe("/Users/test/project");
    expect(typeof t.isActive).toBe("boolean");
    expect(typeof t.isRateLimited).toBe("boolean");
    expect(typeof t.isPromoted).toBe("boolean");
  });

  it("shows currentTask for non-claude agents", () => {
    const id = terminalsStore.add({
      name: "Terminal 2",
      sessionId: "sess2",
      cwd: null,
      fontSize: 14,
      awaitingInput: null,
      agentType: "aider",
    });
    terminalsStore.update(id, { currentTask: "Running migration" });

    const snap = buildActivitySnapshot();
    expect(snap.terminals[0].currentTask).toBe("Running migration");
  });

  it("reflects isPromoted from globalWorkspaceStore", () => {
    const id = terminalsStore.add({
      name: "Terminal 3",
      sessionId: null,
      cwd: null,
      fontSize: 14,
      awaitingInput: null,
    });
    globalWorkspaceStore.togglePromote(id);

    const snap = buildActivitySnapshot();
    expect(snap.terminals[0].isPromoted).toBe(true);
  });
});
