import { render } from "@solidjs/testing-library";
import { listen as tauriListen } from "@tauri-apps/api/event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import Terminal from "../../components/Terminal/Terminal";
import { __resetModalStackForTest } from "../../stores/modalStack";
import { terminalsStore } from "../../stores/terminals";

// Mock pty hooks
const ptyMocks = vi.hoisted(() => ({
	sendCommand: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("../../hooks/usePty", () => ({
	usePty: () => ({
		sendCommand: ptyMocks.sendCommand,
	}),
}));

vi.mock("../../stores/appLogger", () => ({
	appLogger: {
		warn: vi.fn(),
		error: vi.fn(),
		info: vi.fn(),
		debug: vi.fn(),
	},
}));

vi.mock("../../hooks/useAppearanceSync", () => ({
	useAppearanceSync: () => ({}),
}));

vi.mock("../../hooks/useTerminalLifecycle", () => ({
	useTerminalLifecycle: () => ({
		handleClose: vi.fn(),
		handleRename: vi.fn(),
		handleDuplicate: vi.fn(),
		handleDetach: vi.fn(),
		handleTabContextMenu: vi.fn(),
	}),
}));

vi.mock("../../hooks/useTerminalNotifications", () => ({
	useTerminalNotifications: () => ({}),
}));

vi.mock("../../hooks/useTerminalAudio", () => ({
	useTerminalAudio: () => ({
		handleBell: vi.fn(),
	}),
}));

vi.mock("../../invoke", () => ({
	invoke: vi.fn().mockResolvedValue({}),
	listen: vi.fn().mockResolvedValue(vi.fn()),
}));

vi.mock("../../stores/notifications", () => ({
	notificationsStore: {
		play: vi.fn().mockResolvedValue(undefined),
		playCompletion: vi.fn().mockResolvedValue(undefined),
		state: { config: { silence_remote_completions: false } },
	},
}));

vi.mock("../../components/Terminal/CanvasTerminal", () => ({
	default: (props: Record<string, unknown>) => <div data-testid="canvas-terminal" {...props} />,
}));

vi.mock("../../components/Terminal/TerminalSearch", () => ({
	TerminalSearch: () => <div data-testid="terminal-search" />,
}));

vi.mock("../../components/Sidebar/ComposePanel", () => ({
	ComposePanel: () => <div data-testid="compose-panel" />,
}));

/**
 * Regression tests for the remote-tab exit race fix: Terminal.tsx's own
 * `subscribePty` exit callback must defer entirely to `useAppInit.ts`'s
 * `session-closed` listener for a remote tab, doing only its own local
 * component teardown. These tests exercise ONLY Terminal.tsx's own half in
 * isolation (no `session-closed` event is fired), since that's the side
 * under change here. The listener's own half (agentType/resume-banner
 * clearing, the agent-stopped notification, the auto-close countdown) is
 * covered separately in `useAppInit.test.ts`.
 *
 * No test here mounts BOTH halves together and fires both events against one
 * shared session — a true combined-ownership integration test. `initApp` is a
 * real app-startup entry point (theme loading, a 30s snapshot `setInterval`,
 * ~15 other listeners) with no teardown hook, and getting it to coexist with
 * a mounted `<Terminal>` under fake timers (needed to avoid leaking that
 * `setInterval`) turned out to need more mock surface than is worth building
 * for this: `subscribePty`'s `pty-exit-*` registration never completed under
 * fake timers in that combination, for a reason not tracked down. Confidence
 * that the combination is safe rests on the isolation tests above plus
 * `Terminal.tsx`'s `isRemoteTab` guard: for a remote tab, EVERY store mutation
 * in the pty-exit callback is now skipped, so firing `session-closed` before,
 * after, or interleaved with `pty-exit` reaches the exact same code (only
 * `useAppInit.ts`'s listener ever touches the store), which is what removes
 * the race rather than narrowing its window.
 */
describe("Terminal remote-tab exit (subscribePty callback)", () => {
	function captureExitHandler(sessionId: string) {
		let handler: (() => void) | null = null;
		vi.mocked(tauriListen).mockImplementation((async (event: string, cb: () => void) => {
			if (event === `pty-exit-${sessionId}`) handler = cb;
			return vi.fn();
		}) as unknown as typeof tauriListen);
		return {
			hasHandler: () => handler !== null,
			fire: () => handler?.(),
		};
	}

	beforeEach(() => {
		__resetModalStackForTest();
		for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
	});

	afterEach(() => {
		__resetModalStackForTest();
		for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
		vi.clearAllMocks();
	});

	it("does NOT call notifyShellExit for a remote tab with no agent (session-closed owns it instead)", async () => {
		const sessionId = "remote-shell-sess";
		const exit = captureExitHandler(sessionId);
		const id = terminalsStore.add({
			name: "Remote Shell",
			sessionId,
			fontSize: 14,
			cwd: "/repo",
			awaitingInput: null,
			isRemote: true,
		});
		render(() => <Terminal id={id} />);

		const notifySpy = vi.spyOn(terminalsStore, "notifyShellExit");
		// subscribePty()'s chain of dynamic import + two `listen()` awaits takes
		// a handful of microtask ticks to register the pty-exit handler.
		await vi.waitFor(() => {
			if (!exit.hasHandler()) throw new Error("pty-exit handler not registered yet");
		});
		exit.fire();

		expect(notifySpy).not.toHaveBeenCalled();
		notifySpy.mockRestore();
	});

	it("leaves agentType/sessionId/shellState untouched for a remote agent tab (session-closed owns the teardown)", async () => {
		const sessionId = "remote-agent-sess";
		const exit = captureExitHandler(sessionId);
		const id = terminalsStore.add({
			name: "Remote Agent",
			sessionId,
			fontSize: 14,
			cwd: "/repo",
			awaitingInput: null,
			isRemote: true,
			agentType: "claude",
		});
		render(() => <Terminal id={id} />);

		await vi.waitFor(() => {
			if (!exit.hasHandler()) throw new Error("pty-exit handler not registered yet");
		});
		exit.fire();

		// Terminal.tsx must not mutate the shared store for a remote tab —
		// useAppInit.ts's session-closed listener (which reads its OWN payload's
		// agent_type) is the sole owner of this teardown now.
		const terminal = terminalsStore.get(id);
		expect(terminal?.agentType).toBe("claude");
		expect(terminal?.sessionId).toBe(sessionId);
		expect(terminal?.shellState).not.toBe("exited");
	});

	it("still calls notifyShellExit immediately for a non-remote tab with no agent (unchanged local behavior)", async () => {
		const sessionId = "local-shell-sess";
		const exit = captureExitHandler(sessionId);
		const id = terminalsStore.add({
			name: "Local Shell",
			sessionId,
			fontSize: 14,
			cwd: "/repo",
			awaitingInput: null,
			isRemote: false,
		});
		render(() => <Terminal id={id} />);

		const notifySpy = vi.spyOn(terminalsStore, "notifyShellExit");
		await vi.waitFor(() => {
			if (!exit.hasHandler()) throw new Error("pty-exit handler not registered yet");
		});
		exit.fire();

		expect(notifySpy).toHaveBeenCalledWith(id);
		notifySpy.mockRestore();
	});

	it("still locally wipes agentType/sessionId for a non-remote agent tab (unchanged local behavior)", async () => {
		const sessionId = "local-agent-sess";
		const exit = captureExitHandler(sessionId);
		const id = terminalsStore.add({
			name: "Local Agent",
			sessionId,
			fontSize: 14,
			cwd: "/repo",
			awaitingInput: null,
			isRemote: false,
			agentType: "claude",
		});
		render(() => <Terminal id={id} />);

		await vi.waitFor(() => {
			if (!exit.hasHandler()) throw new Error("pty-exit handler not registered yet");
		});
		exit.fire();

		const terminal = terminalsStore.get(id);
		expect(terminal?.agentType).toBeNull();
		expect(terminal?.sessionId).toBeNull();
		expect(terminal?.shellState).toBe("exited");
	});
});
