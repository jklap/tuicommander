import { render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import Terminal from "../../components/Terminal/Terminal";
import { __resetModalStackForTest } from "../../stores/modalStack";
import { terminalsStore } from "../../stores/terminals";

// Same mock surface as Terminal-remoteExit.test.tsx (its sibling): the real
// component with everything heavy around it stubbed.
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

// Capture the options Terminal.tsx hands subscribePty so a test can deliver a
// backend session-state snapshot through `onStateChange` — the browser-mode
// (WebSocket) path, which has no Tauri `session-state-changed` event behind it.
const transportMocks = vi.hoisted(() => ({
	opts: null as null | { onStateChange?: (state: Record<string, unknown>) => void },
}));

vi.mock("../../transport", async (importOriginal) => {
	const actual = await importOriginal<typeof import("../../transport")>();
	return {
		...actual,
		subscribePty: vi.fn(async (_sessionId: string, _onData: unknown, _onExit: unknown, opts: unknown) => {
			transportMocks.opts = opts as typeof transportMocks.opts;
			return () => {};
		}),
	};
});

/**
 * Characterization tests for how Terminal.tsx's `subscribePty` `onStateChange`
 * snapshot (the WebSocket frame browser/PWA clients receive) maps the backend
 * `declared_background_work` field onto the terminal store. The desktop path
 * (`useAgentPolling`'s `applySessionState`) is covered in
 * `hooks/useAgentPolling.test.ts`; this is the browser-mode twin, and the two
 * must agree.
 */
describe("Terminal onStateChange (browser-mode session-state snapshot)", () => {
	beforeEach(() => {
		__resetModalStackForTest();
		transportMocks.opts = null;
		for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
	});

	afterEach(() => {
		__resetModalStackForTest();
		for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
		vi.clearAllMocks();
	});

	async function mountWithSession(sessionId: string) {
		const id = terminalsStore.add({
			name: "Agent",
			sessionId,
			fontSize: 14,
			cwd: "/repo",
			awaitingInput: null,
			agentType: "claude",
		});
		render(() => <Terminal id={id} />);
		await vi.waitFor(() => {
			if (!transportMocks.opts?.onStateChange) throw new Error("onStateChange not registered yet");
		});
		return id;
	}

	it("mirrors declared_background_work onto the terminal, independent of raw shell busy and backgroundWork", async () => {
		const id = await mountWithSession("sess-declared");

		transportMocks.opts?.onStateChange?.({
			shell_state: "idle",
			agent_state: "working",
			declared_background_work: true,
		});

		expect(terminalsStore.get(id)?.declaredBackgroundWork).toBe(true);
		expect(terminalsStore.get(id)?.backgroundWork).toBe(false);
		expect(terminalsStore.isWorking(id)).toBe(true);
		expect(terminalsStore.isBusy(id)).toBe(false);
	});

	it("retracts declaredBackgroundWork when a later snapshot omits the field (serde skips false)", async () => {
		const id = await mountWithSession("sess-retract");

		transportMocks.opts?.onStateChange?.({
			shell_state: "idle",
			agent_state: "working",
			declared_background_work: true,
		});
		expect(terminalsStore.get(id)?.declaredBackgroundWork).toBe(true);

		transportMocks.opts?.onStateChange?.({ shell_state: "idle", agent_state: "idle" });
		expect(terminalsStore.get(id)?.declaredBackgroundWork).toBe(false);
		expect(terminalsStore.isWorking(id)).toBe(false);
	});

	it("treats a non-boolean declared_background_work as not declared (strict === true)", async () => {
		const id = await mountWithSession("sess-strict");

		transportMocks.opts?.onStateChange?.({
			shell_state: "idle",
			agent_state: "working",
			declared_background_work: "true",
		});

		expect(terminalsStore.get(id)?.declaredBackgroundWork).toBe(false);
	});
});
