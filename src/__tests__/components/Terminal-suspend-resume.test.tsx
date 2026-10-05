import { render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { createSession, resize, sendCommand, mockRpc, ptyOptions, mockVerifyResume } = vi.hoisted(() => ({
	createSession: vi.fn().mockResolvedValue("fresh-session"),
	resize: vi.fn().mockResolvedValue(undefined),
	sendCommand: vi.fn().mockResolvedValue(undefined),
	mockRpc: vi.fn(),
	ptyOptions: new Map<
		string,
		{
			onStateChange?: (state: Record<string, unknown>) => void;
			onParsed?: (frame: { type: string; event: unknown }) => void;
		}
	>(),
	mockVerifyResume: vi.fn(),
}));

vi.mock("../../hooks/usePty", () => ({
	usePty: () => ({
		createSession,
		resize,
		sendCommand,
		close: vi.fn(),
		getKittyFlags: vi.fn().mockResolvedValue(0),
	}),
}));
vi.mock("../../transport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../transport")>()),
	rpc: (...args: unknown[]) => mockRpc(...args),
	subscribePty: (
		sessionId: string,
		_onData: unknown,
		_onExit: unknown,
		options: typeof ptyOptions extends Map<string, infer V> ? V : never,
	) => {
		ptyOptions.set(sessionId, options);
		return Promise.resolve(() => {});
	},
}));
// The mounted tab reconnects to a live PTY and asks which process runs in the foreground.
vi.mock("@tauri-apps/api/core", () => ({
	invoke: vi.fn(async (cmd: string) => (cmd === "get_session_foreground_process" ? "claude" : undefined)),
}));
vi.mock("../../utils/agentSession", () => ({
	verifyAndBuildResumeCommand: (...args: unknown[]) => mockVerifyResume(...args),
}));
vi.mock("../../stores/agentConfigs", () => ({
	ensureAgentConfigsForRepo: vi.fn().mockResolvedValue({ getEnvFlags: () => ({}) }),
	agentConfigsForRepo: () => ({ getEnvFlags: () => ({}) }),
}));
vi.mock("../../components/Terminal/glyphCache", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../components/Terminal/glyphCache")>()),
	getSharedMetrics: () => ({ cellWidth: 8, cellHeight: 16 }),
}));
vi.mock("../../components/Terminal/CanvasTerminal", () => ({
	default: () => <div data-testid="canvas" />,
}));

import { Terminal } from "../../components/Terminal/Terminal";
import { terminalsStore } from "../../stores/terminals";
import { resumeTerminal, suspendTerminal } from "../../utils/suspendTerminal";

function openIdleAgentTab() {
	const id = terminalsStore.add({
		sessionId: "live-session",
		cwd: "/Gits/alpha",
		repoPath: "/Gits/alpha",
		name: "agent",
		fontSize: 13,
		awaitingInput: null,
	});
	terminalsStore.update(id, {
		agentType: "claude",
		agentSessionId: "agent-uuid",
		tuicSession: "tab-uuid",
		shellState: "idle",
		agentState: "idle",
	});
	const oldRaf = globalThis.requestAnimationFrame;
	const width = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "offsetWidth");
	const height = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "offsetHeight");
	Object.defineProperty(HTMLElement.prototype, "offsetWidth", { configurable: true, get: () => 800 });
	Object.defineProperty(HTMLElement.prototype, "offsetHeight", { configurable: true, get: () => 600 });
	globalThis.requestAnimationFrame = (cb) => {
		cb(0);
		return 1;
	};
	try {
		render(() => <Terminal id={id} cwd="/Gits/alpha" alwaysVisible />);
	} finally {
		globalThis.requestAnimationFrame = oldRaf;
		if (width) Object.defineProperty(HTMLElement.prototype, "offsetWidth", width);
		if (height) Object.defineProperty(HTMLElement.prototype, "offsetHeight", height);
	}
	return id;
}

const shellIdle = (sessionId: string) =>
	ptyOptions.get(sessionId)?.onParsed?.({ type: "parsed", event: { type: "shell-state", state: "idle" } });

describe("Resume of a suspended agent tab", () => {
	beforeEach(() => {
		createSession.mockClear().mockResolvedValue("fresh-session");
		sendCommand.mockClear().mockResolvedValue(undefined);
		resize.mockClear().mockResolvedValue(undefined);
		mockRpc.mockReset().mockResolvedValue(undefined);
		mockVerifyResume.mockReset().mockResolvedValue("claude --resume agent-uuid");
		ptyOptions.clear();
	});

	// Catches: Resume opens the PTY but never types the agent's resume command (the user gets a
	// bare shell), or types it again on every later idle (the agent is relaunched inside itself).
	it("types the resume command once, when the new shell first goes idle", async () => {
		const id = openIdleAgentTab();
		await waitFor(() => expect(resize).toHaveBeenCalledWith("live-session", expect.any(Number), expect.any(Number)));
		expect((await suspendTerminal(id)).ok).toBe(true);

		expect((await resumeTerminal(id)).ok).toBe(true);
		await waitFor(() => expect(terminalsStore.get(id)?.sessionId).toBe("fresh-session"));
		await waitFor(() => expect(ptyOptions.get("fresh-session")?.onParsed).toBeDefined());
		expect(sendCommand).not.toHaveBeenCalled();

		shellIdle("fresh-session");
		shellIdle("fresh-session");

		expect(sendCommand).toHaveBeenCalledTimes(1);
		expect(sendCommand).toHaveBeenCalledWith("fresh-session", "claude --resume agent-uuid", null);
		expect(createSession).toHaveBeenCalledWith(expect.objectContaining({ agent_type: "claude" }));
	});
});

describe("Pending agent launch readiness", () => {
	const openedTabs: string[] = [];
	afterEach(() => {
		for (const id of openedTabs.splice(0)) terminalsStore.remove(id);
	});
	beforeEach(() => {
		sendCommand.mockClear().mockResolvedValue(undefined);
		resize.mockClear().mockResolvedValue(undefined);
		mockRpc.mockReset().mockResolvedValue(undefined);
		ptyOptions.clear();
	});

	// Catches: asynchronous command preparation completes after the only idle event.
	it("launches once when the command becomes ready after the shell is idle", async () => {
		const id = openIdleAgentTab();
		openedTabs.push(id);
		await waitFor(() => expect(ptyOptions.has("live-session")).toBe(true));
		terminalsStore.update(id, { pendingInitCommand: "claude" });
		await waitFor(() => expect(sendCommand).toHaveBeenCalledWith("live-session", "claude", null));
		shellIdle("live-session");
		expect(sendCommand).toHaveBeenCalledTimes(1);
	});

	// Catches: the remote prompt predates subscription and only its WS snapshot remains.
	it("launches from an idle state snapshot without a parsed prompt event", async () => {
		const id = openIdleAgentTab();
		openedTabs.push(id);
		await waitFor(() => expect(ptyOptions.has("live-session")).toBe(true));
		terminalsStore.update(id, { shellState: null, pendingInitCommand: "claude" });
		expect(sendCommand).not.toHaveBeenCalled();
		ptyOptions.get("live-session")?.onStateChange?.({ shell_state: "idle" });
		await waitFor(() => expect(sendCommand).toHaveBeenCalledWith("live-session", "claude", null));
		ptyOptions.get("live-session")?.onStateChange?.({ shell_state: "idle" });
		expect(sendCommand).toHaveBeenCalledTimes(1);
	});

	// Catches: pending launch input is typed into a running shell command.
	it("keeps the pending command until the busy shell becomes idle", async () => {
		const id = openIdleAgentTab();
		openedTabs.push(id);
		await waitFor(() => expect(ptyOptions.has("live-session")).toBe(true));
		terminalsStore.update(id, { shellState: "busy", pendingInitCommand: "claude" });
		expect(sendCommand).not.toHaveBeenCalled();
		shellIdle("live-session");
		await waitFor(() => expect(sendCommand).toHaveBeenCalledWith("live-session", "claude", null));
	});
});
