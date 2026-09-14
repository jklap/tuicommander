import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";

const { mockRpc } = vi.hoisted(() => ({ mockRpc: vi.fn().mockResolvedValue(undefined) }));

vi.mock("../../transport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../transport")>()),
	rpc: mockRpc,
}));

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
		handleWorktreeSetupScriptCompleted: vi.fn(),
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

type Handler = (event: { payload: unknown }) => void;

async function start() {
	const handlers = new Map<string, Handler>();
	vi.mocked(listen).mockImplementation(((event: string, h: Handler) => {
		handlers.set(event, h);
		return Promise.resolve(vi.fn());
	}) as unknown as typeof listen);
	await initApp(createMockDeps());
	return handlers;
}

const answers = () => mockRpc.mock.calls.filter(([cmd]) => cmd === "session_suspend_response");
const flush = async () => {
	for (let i = 0; i < 6; i++) await Promise.resolve();
};

describe("session-suspend-requested listener (critic 1358 r2)", () => {
	beforeEach(() => {
		vi.useFakeTimers();
		vi.mocked(listen).mockReset().mockResolvedValue(vi.fn());
		mockRpc.mockReset().mockResolvedValue(undefined);
		for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
	});
	afterEach(() => {
		vi.restoreAllMocks();
		vi.useRealTimers();
		repositoriesStore._testCancelPendingSave();
		paneLayoutStore._testCancelPendingSave();
	});

	function addLiveShell(sessionId: string): string {
		const id = terminalsStore.add({
			sessionId,
			cwd: "/Gits/alpha",
			name: "sh",
			fontSize: 14,
			awaitingInput: null,
		});
		terminalsStore.update(id, { shellState: "idle" });
		return id;
	}

	// Catches: the tab suspending but never answering, so the MCP caller waits the full 20 s
	// and reports "no tab answered" for a suspend that worked.
	it("answers ok with the request id once the tab suspended", async () => {
		const handlers = await start();
		const id = addLiveShell("sess-a");
		handlers.get("session-suspend-requested")!({ payload: { session_id: "sess-a", request_id: "req-A" } });
		await flush();
		expect(terminalsStore.get(id)?.suspended).toBe(true);
		expect(answers()).toEqual([["session_suspend_response", { requestId: "req-A", ok: true, reason: null }]]);
	});

	// Catches: a refusal answered as ok, or answered without the reason the MCP caller reports.
	it("answers the tab's refusal reason", async () => {
		const handlers = await start();
		const id = addLiveShell("sess-b");
		terminalsStore.update(id, { shellState: "busy" });
		handlers.get("session-suspend-requested")!({ payload: { session_id: "sess-b", request_id: "req-B" } });
		await flush();
		expect(terminalsStore.get(id)?.suspended).toBe(false);
		expect(answers()).toEqual([
			["session_suspend_response", { requestId: "req-B", ok: false, reason: "command running" }],
		]);
	});

	// Catches: a client that does not own the session answering "unknown tab" and so winning the
	// race against the client that does (the backend takes the first verdict).
	it("stays silent for a session it has no tab for", async () => {
		const handlers = await start();
		handlers.get("session-suspend-requested")!({ payload: { session_id: "not-mine", request_id: "req-C" } });
		await flush();
		expect(answers()).toEqual([]);
	});

	// Catches: a mirrored event (carries __tuic_origin) being acted on as well as the original,
	// closing the PTY twice and answering twice.
	it("ignores an event that carries a transport origin", async () => {
		const handlers = await start();
		const id = addLiveShell("sess-d");
		handlers.get("session-suspend-requested")!({
			payload: { session_id: "sess-d", request_id: "req-D", __tuic_origin: "remote" },
		});
		await flush();
		expect(terminalsStore.get(id)?.suspended).toBe(false);
		expect(answers()).toEqual([]);
	});

	// Catches: a failing answer RPC rejecting unhandled (unhandled rejection kills the test run
	// and, in the app, logs nothing) instead of being logged.
	it("survives a failing answer request", async () => {
		const handlers = await start();
		addLiveShell("sess-e");
		mockRpc.mockImplementation((cmd: string) =>
			cmd === "session_suspend_response" ? Promise.reject(new Error("offline")) : Promise.resolve(undefined),
		);
		handlers.get("session-suspend-requested")!({ payload: { session_id: "sess-e", request_id: "req-E" } });
		await flush();
		expect(answers()).toHaveLength(1);
	});

	// Catches: session-closed (sent by the backend as soon as close_pty ran) arriving while the
	// suspend is still in flight, before `suspended` is set: the MCP-spawned tab would start its
	// auto-close countdown and be deleted although the suspend then succeeds.
	it("does not auto-remove an MCP-spawned tab whose suspend is still closing the PTY", async () => {
		const handlers = await start();
		handlers.get("session-created")!({ payload: { session_id: "mcp-r2", cwd: null, agent_type: null } });
		const id = terminalsStore.getIds().find((t) => terminalsStore.get(t)?.sessionId === "mcp-r2")!;
		let release: () => void = () => {};
		mockRpc.mockImplementation((cmd: string) =>
			cmd === "close_pty"
				? new Promise<void>((resolve) => {
						release = resolve;
					})
				: Promise.resolve(undefined),
		);
		handlers.get("session-suspend-requested")!({ payload: { session_id: "mcp-r2", request_id: "req-F" } });
		await flush();

		handlers.get("session-closed")!({ payload: { session_id: "mcp-r2", reason: "close_requested", agent_type: null } });
		release();
		await flush();
		vi.advanceTimersByTime(60_000);

		expect(terminalsStore.get(id)).toBeDefined();
		expect(terminalsStore.get(id)?.suspended).toBe(true);
		expect(terminalsStore.get(id)?.name).not.toMatch(/\(\d+s\)/);
	});
});
