import { fireEvent, render } from "@solidjs/testing-library";
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

// Mock other dependencies
vi.mock("../../stores/appLogger", () => ({
	appLogger: {
		warn: vi.fn(),
		error: vi.fn(),
		info: vi.fn(),
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
}));

vi.mock("../../stores/settings", () => ({
	settingsStore: {
		state: {
			appearance: {
				theme: "dark",
			},
			terminal: {
				copyOnSelect: false,
			},
		},
	},
}));

vi.mock("../../stores/notifications", () => ({
	notificationsStore: {
		play: vi.fn().mockResolvedValue(undefined),
	},
}));

// Mock other complex components to avoid deep dependency chains
vi.mock("../../components/Terminal/CanvasTerminal", () => ({
	default: (props: Record<string, unknown>) => <div data-testid="canvas-terminal" {...props} />,
}));

vi.mock("../../components/Terminal/TerminalSearch", () => ({
	TerminalSearch: () => <div data-testid="terminal-search" />,
}));

vi.mock("../../components/Sidebar/ComposePanel", () => ({
	ComposePanel: () => <div data-testid="compose-panel" />,
}));

describe("Terminal resume banner", () => {
	beforeEach(() => {
		__resetModalStackForTest();
		// Clear all terminals
		for (const id of terminalsStore.getIds()) {
			terminalsStore.remove(id);
		}
		vi.clearAllMocks();
	});

	afterEach(() => {
		__resetModalStackForTest();
		for (const id of terminalsStore.getIds()) {
			terminalsStore.remove(id);
		}
	});

	const addTerminalWithResume = (resumeCommand: string | null) => {
		const id = terminalsStore.add({
			name: "Test Terminal",
			sessionId: "sess-123",
			fontSize: 14,
			cwd: "/test",
			awaitingInput: null,
		});
		terminalsStore.update(id, { pendingResumeCommand: resumeCommand });
		return id;
	};

	it("renders resume banner when pendingResumeCommand is set", () => {
		const id = addTerminalWithResume("claude --continue");
		const { container } = render(() => <Terminal id={id} />);

		const banner = container.querySelector(".resumeBanner");
		expect(banner).not.toBeNull();
		expect(banner?.textContent).toContain("Agent session was active — click to resume");
	});

	it("does not render resume banner when pendingResumeCommand is null", () => {
		const id = addTerminalWithResume(null);
		const { container } = render(() => <Terminal id={id} />);

		const banner = container.querySelector(".resumeBanner");
		expect(banner).toBeNull();
	});

	it("clicking resume banner calls pty.sendCommand and clears pendingResumeCommand", async () => {
		const id = addTerminalWithResume("claude --continue");
		const { container } = render(() => <Terminal id={id} />);

		const banner = container.querySelector(".resumeBanner");
		expect(banner).not.toBeNull();

		fireEvent.click(banner!);

		// Should call sendCommand with the session id and resume command
		expect(ptyMocks.sendCommand).toHaveBeenCalledWith("sess-123", "claude --continue", null);

		// Should clear the pendingResumeCommand
		expect(terminalsStore.get(id)?.pendingResumeCommand).toBeNull();
	});

	it("clicking dismiss button clears pendingResumeCommand without calling sendCommand", async () => {
		const id = addTerminalWithResume("claude --continue");
		const { container } = render(() => <Terminal id={id} />);

		const dismissButton = container.querySelector(".resumeDismiss");
		expect(dismissButton).not.toBeNull();

		fireEvent.click(dismissButton!);

		// Should NOT call sendCommand
		expect(ptyMocks.sendCommand).not.toHaveBeenCalled();

		// Should clear the pendingResumeCommand
		expect(terminalsStore.get(id)?.pendingResumeCommand).toBeNull();
	});

	it("dismiss button prevents event propagation (does not also trigger resume)", async () => {
		const id = addTerminalWithResume("claude --continue");
		const { container } = render(() => <Terminal id={id} />);

		const dismissButton = container.querySelector(".resumeDismiss");
		expect(dismissButton).not.toBeNull();

		// Create a click event to verify preventDefault/stopPropagation behavior
		const clickEvent = new MouseEvent("click", { bubbles: true });
		const preventDefaultSpy = vi.spyOn(clickEvent, "preventDefault");
		const stopPropagationSpy = vi.spyOn(clickEvent, "stopPropagation");

		fireEvent(dismissButton!, clickEvent);

		expect(preventDefaultSpy).toHaveBeenCalled();
		expect(stopPropagationSpy).toHaveBeenCalled();
		expect(ptyMocks.sendCommand).not.toHaveBeenCalled();
		expect(terminalsStore.get(id)?.pendingResumeCommand).toBeNull();
	});

	it("does not call sendCommand when sessionId is missing", () => {
		const id = terminalsStore.add({
			name: "Test Terminal",
			sessionId: null, // No session ID
			fontSize: 14,
			cwd: "/test",
			awaitingInput: null,
		});
		terminalsStore.update(id, { pendingResumeCommand: "claude --continue" });

		const { container } = render(() => <Terminal id={id} />);
		const banner = container.querySelector(".resumeBanner");

		fireEvent.click(banner!);

		// Should not call sendCommand when sessionId is null
		expect(ptyMocks.sendCommand).not.toHaveBeenCalled();

		// handleResume's guard is `if (cmd && sessionId)` — with no sessionId,
		// the whole branch (including the clear) is skipped, so the banner
		// stays exactly as it was rather than silently vanishing with nothing
		// having been sent.
		expect(terminalsStore.get(id)?.pendingResumeCommand).toBe("claude --continue");
	});

	// Code-review finding (2026-09-29): the original test suite here only ever
	// exercised the pre-existing generic-fallback-text/click/dismiss behavior —
	// the title-in-banner-text rendering and the exit-vs-restore
	// `pendingResumeIsClickOnly` wiring this diff actually added had zero
	// coverage.
	describe("title and pendingResumeIsClickOnly wiring", () => {
		const addTerminalWithResumeDetails = (source: "restore" | "exit" | null, title: string | null) => {
			const id = terminalsStore.add({
				name: "Test Terminal",
				sessionId: "sess-123",
				fontSize: 14,
				cwd: "/test",
				awaitingInput: null,
			});
			terminalsStore.update(id, {
				pendingResumeCommand: "claude --resume abc123",
				pendingResumeTitle: title,
				pendingResumeSource: source,
			});
			return id;
		};

		it("shows the title in the banner text and tooltip when pendingResumeTitle is set", () => {
			const id = addTerminalWithResumeDetails("exit", "file-locations");
			const { container } = render(() => <Terminal id={id} />);

			const banner = container.querySelector(".resumeBanner");
			expect(banner?.textContent).toContain('Resume "file-locations" — click to resume');
			const textSpan = banner?.querySelector("span");
			expect(textSpan?.getAttribute("title")).toBe("file-locations");
		});

		it("falls back to the generic banner text when pendingResumeTitle is null", () => {
			const id = addTerminalWithResumeDetails("exit", null);
			const { container } = render(() => <Terminal id={id} />);

			const banner = container.querySelector(".resumeBanner");
			expect(banner?.textContent).toContain("Agent session was active — click to resume");
		});

		it("passes pendingResumeIsClickOnly=true to CanvasTerminal for an exit-sourced banner", () => {
			const id = addTerminalWithResumeDetails("exit", "file-locations");
			const { container } = render(() => <Terminal id={id} />);

			const canvasTerminal = container.querySelector('[data-testid="canvas-terminal"]');
			expect(canvasTerminal?.getAttribute("pendingresumeisclickonly")).toBe("true");
		});

		it("passes pendingResumeIsClickOnly=false to CanvasTerminal for a restore-sourced banner", () => {
			const id = addTerminalWithResumeDetails("restore", "restored title");
			const { container } = render(() => <Terminal id={id} />);

			const canvasTerminal = container.querySelector('[data-testid="canvas-terminal"]');
			expect(canvasTerminal?.getAttribute("pendingresumeisclickonly")).toBe("false");
		});
	});
});
