import { listen as tauriListen } from "@tauri-apps/api/event";
import { render } from "@solidjs/testing-library";
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

vi.mock("../../stores/settings", () => ({
	settingsStore: {
		state: {
			appearance: { theme: "dark" },
			terminal: { copyOnSelect: false },
		},
	},
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
 * Characterization tests, written before the origin-tracking change lands, for
 * the remote-tab exit race described in the plan: Terminal.tsx's own
 * `subscribePty` exit callback and `useAppInit.ts`'s `session-closed` listener
 * both act on a remote tab today. These tests exercise ONLY Terminal.tsx's own
 * half in isolation (no `session-closed` event is fired), since that's the
 * side under change here.
 */
describe("Terminal remote-tab exit (subscribePty callback)", () => {
	function captureExitHandler(sessionId: string) {
		let handler: (() => void) | null = null;
		vi.mocked(tauriListen).mockImplementation((async (event: string, cb: () => void) => {
			if (event === `pty-exit-${sessionId}`) handler = cb;
			return vi.fn();
		}) as typeof tauriListen);
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

	it("calls notifyShellExit immediately for a remote tab with no agent (today's race)", async () => {
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

		expect(notifySpy).toHaveBeenCalledWith(id);
		notifySpy.mockRestore();
	});

	it("locally wipes agentType/sessionId for a remote agent tab today, pre-empting session-closed's own countdown", async () => {
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

		// Today: Terminal.tsx's own hadAgent branch nulls these immediately,
		// regardless of isRemote — before session-closed's countdown (which
		// reads its OWN payload's agent_type, not the store) ever gets a
		// chance to run against a still-intact tab.
		const terminal = terminalsStore.get(id);
		expect(terminal?.agentType).toBeNull();
		expect(terminal?.sessionId).toBeNull();
		expect(terminal?.shellState).toBe("exited");
	});
});
