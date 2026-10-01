import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";

import { listen } from "@tauri-apps/api/event";
import { type AppInitDeps, initApp } from "../../hooks/useAppInit";
import { paneLayoutStore } from "../../stores/paneLayout";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";

function createMockDeps(): AppInitDeps {
	return {
		pty: { listActiveSessions: vi.fn().mockResolvedValue([]), close: vi.fn().mockResolvedValue(undefined) },
		setQuitDialogVisible: vi.fn(),
		setStatusInfo: vi.fn(),
		handleBranchSelect: vi.fn().mockResolvedValue(undefined),
		refreshAllBranchStats: vi.fn(),
		getDefaultFontSize: () => 14,
		stores: {
			hydrate: vi.fn().mockResolvedValue(undefined),
			startPolling: vi.fn(),
			stopPolling: vi.fn(),
			startAutoFetch: vi.fn(),
			startPrNotificationTimer: vi.fn(),
			loadFontFromConfig: vi.fn(),
			refreshDictationConfig: vi.fn().mockResolvedValue(undefined),
			startUserActivityListening: vi.fn(),
		},
		applyPlatformClass: vi.fn().mockReturnValue("macos"),
		onCloseRequested: vi.fn().mockResolvedValue(undefined),
		registerRepo: vi.fn().mockResolvedValue(undefined),
	};
}

describe("session-closed after a suspend (critic 1358)", () => {
	beforeEach(() => {
		vi.useFakeTimers();
		vi.mocked(listen).mockReset().mockResolvedValue(vi.fn());
		for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
	});
	afterEach(() => {
		vi.restoreAllMocks();
		vi.useRealTimers();
		repositoriesStore._testCancelPendingSave();
		paneLayoutStore._testCancelPendingSave();
	});

	// Catches: an MCP-spawned (remote) tab that is suspended gets its backend session-closed
	// event, which starts the remote auto-close countdown and removes the tab the user kept.
	it("does not auto-remove a suspended MCP-spawned tab", async () => {
		const handlers = new Map<string, (event: { payload: unknown }) => void>();
		vi.mocked(listen).mockImplementation(((event: string, h: (event: { payload: unknown }) => void) => {
			handlers.set(event, h);
			return Promise.resolve(vi.fn());
		}) as unknown as typeof listen);
		await initApp(createMockDeps());

		handlers.get("session-created")!({ payload: { session_id: "mcp-sess", cwd: null, agent_type: "claude" } });
		const termId = terminalsStore.getIds().find((id) => terminalsStore.get(id)?.sessionId === "mcp-sess")!;
		expect(termId).toBeDefined();
		const name = terminalsStore.get(termId)!.name;

		// The state suspendTerminal leaves behind once close_pty resolved.
		terminalsStore.update(termId, { suspended: true });
		terminalsStore.setSessionId(termId, null);
		handlers.get("session-closed")!({
			payload: { session_id: "mcp-sess", reason: "close_requested", agent_type: "claude" },
		});

		vi.advanceTimersByTime(60_000);
		expect(terminalsStore.get(termId)).toBeDefined();
		expect(terminalsStore.get(termId)?.name).toBe(name);
		expect(terminalsStore.get(termId)?.suspended).toBe(true);
	});
});
