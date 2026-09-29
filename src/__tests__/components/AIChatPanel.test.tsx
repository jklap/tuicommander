// @vitest-environment jsdom
//
// The panel renders an agent's answer through ContentRenderer, whose DOMPurify
// pass needs a complete NodeIterator; happy-dom's is not.

import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { cleanup, render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { mockDetachPanel, mockReattachPanel, mockClosePanel } = vi.hoisted(() => ({
	mockDetachPanel: vi.fn().mockResolvedValue(undefined),
	mockReattachPanel: vi.fn().mockResolvedValue(undefined),
	mockClosePanel: vi.fn().mockResolvedValue(undefined),
}));

const { mockWriteClipboard, mockOpenFile, mockOpenUrl } = vi.hoisted(() => ({
	mockWriteClipboard: vi.fn().mockResolvedValue(undefined),
	mockOpenFile: vi.fn(),
	mockOpenUrl: vi.fn(),
}));

vi.mock("../../utils/clipboard", () => ({ writeClipboard: mockWriteClipboard }));
vi.mock("../../utils/filePreview", () => ({ openTerminalFilePath: mockOpenFile }));
vi.mock("../../utils/openUrl", () => ({ handleOpenUrl: mockOpenUrl }));

vi.mock("@tauri-apps/api/core", () => ({
	invoke: vi.fn().mockResolvedValue(undefined),
	Channel: vi.fn(),
	convertFileSrc: (path: string) => path,
}));

vi.mock("@tauri-apps/api/event", () => ({
	listen: vi.fn().mockResolvedValue(vi.fn()),
	emit: vi.fn().mockResolvedValue(undefined),
	emitTo: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("../../panelRouter", () => ({
	detachPanel: mockDetachPanel,
	reattachPanel: mockReattachPanel,
	closePanel: mockClosePanel,
}));

vi.mock("../../stores/appLogger", () => ({
	appLogger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

vi.mock("../../stores/ui", () => ({
	uiStore: {
		state: { detachedPanels: {} },
		isDetached: vi.fn(() => false),
		setDetached: vi.fn(),
		clearDetached: vi.fn(),
	},
}));

vi.mock("../../transport", () => ({
	isTauri: () => true,
	owningConnectionFor: () => undefined,
}));

// Whether an ego binary is configured is the one setting this panel reads, and
// criterion 9 turns on it being readable as "not configured" rather than as an
// empty string nobody checked.
const settings = vi.hoisted(() => ({ egoExecutable: "/usr/local/bin/ego" }));

vi.mock("../../stores/settings", () => ({
	settingsStore: {
		state: settings,
		isAiChatEnabled: () => true,
		isAcpConfigured: () => settings.egoExecutable.trim().length > 0,
	},
}));

// The client is the IPC boundary and the only thing mocked below it: the store,
// the transcript projection and every reducer between them are the real ones,
// so a test that passes proves the panel reads what the wire actually carries.
const client = vi.hoisted(() => ({
	connect: vi.fn(),
	reconnect: vi.fn(),
	disconnect: vi.fn(),
	newSession: vi.fn(),
	loadSession: vi.fn(),
	listSessions: vi.fn(),
	prompt: vi.fn(),
	cancel: vi.fn(),
	cancelQueued: vi.fn(),
	answerPermission: vi.fn(),
	cancelPermission: vi.fn(),
	answerElicitation: vi.fn(),
	setConfigOption: vi.fn(),
	pause: vi.fn(),
	resumeTurn: vi.fn(),
	compact: vi.fn(),
}));

vi.mock("../../services/acpClient", () => ({ acpClient: client }));

import { invoke } from "@tauri-apps/api/core";
import { emitTo } from "@tauri-apps/api/event";
import { AIChatPanel } from "../../components/AIChatPanel/AIChatPanel";
import { aiChatDraft } from "../../components/AIChatPanel/draft";
import { elicitationFields } from "../../components/AIChatPanel/Interactions";
import { resetAcpChatBindings } from "../../components/AIChatPanel/useAcpChat";
import { aiChatPanelAdapter } from "../../panelAdapters/aiChat";
import { acpStore } from "../../stores/acp";
import { acpTranscript } from "../../stores/acpTranscript";
import { aiChatTabs } from "../../stores/aiChatTabs";
import type {
	AcpAttachmentSnapshot,
	AcpClientEvent,
	AcpConnectionSnapshot,
	AcpSessionConfigOption,
} from "../../types/acp";

const ROOT = "/repo/tuicommander";
const HOME = "/home/boss";
/** Where every chat runs: the whole workspace, never one repository. */
const CHAT_ROOT = "/home/boss/Gits";
const CONNECTION = "01932d5e-0000-7000-8000-0000000000c1";
const SESSION = "01932d5e-0000-7000-8000-0000000000aa";
const SECOND_SESSION = "01932d5e-0000-7000-8000-0000000000bb";
// A real 1x1 PNG admitted by ego's ACP prompt tests.
const PNG_1X1 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=";
let supportsImages = false;

function pasteFile(textarea: HTMLTextAreaElement, file: File): Event {
	const event = new Event("paste", { bubbles: true, cancelable: true });
	Object.defineProperty(event, "clipboardData", {
		value: { items: [{ type: file.type, getAsFile: () => file }] },
	});
	textarea.dispatchEvent(event);
	return event;
}

const MODEL_OPTION: AcpSessionConfigOption = {
	id: "model",
	name: "Model",
	type: "select",
	currentValue: "opus",
	options: [
		{ value: "opus", name: "Opus" },
		{ value: "sonnet", name: "Sonnet" },
	],
};

function attachment(overrides: Partial<AcpAttachmentSnapshot> = {}): AcpAttachmentSnapshot {
	return {
		sessionId: SESSION,
		state: "idle",
		cwd: ROOT,
		additionalDirectories: [],
		configOptions: [MODEL_OPTION],
		usage: null,
		activeTurn: null,
		queuedPrompts: [],
		pendingPermissionIds: [],
		pendingElicitationIds: [],
		...overrides,
	};
}

function snapshot(overrides: Partial<AcpConnectionSnapshot> = {}): AcpConnectionSnapshot {
	return {
		connectionId: CONNECTION,
		generation: 1,
		state: "ready",
		agentInfo: { name: "ego", version: "0.1.0" },
		capabilities: {
			protocol: 1,
			load: true,
			list: true,
			resume: true,
			fork: false,
			delete: false,
			close: true,
			additionalDirectories: true,
			promptImage: supportsImages,
			promptAudio: false,
			promptEmbeddedContext: false,
			mcpStdio: false,
			mcpHttp: true,
			mcpSse: false,
			mcpAcp: true,
			clientFormElicitation: true,
			clientBooleanConfig: false,
			egoHoldVersion: 1,
			egoCompactVersion: 1,
		},
		attachments: [],
		earliestSequence: 1,
		latestSequence: 1,
		settlement: null,
		...overrides,
	};
}

let sequence = 0;

/** One event frame, as the stream would deliver it. */
function feed(event: AcpClientEvent, sessionId: string | null = SESSION, turnId: string | null = null): void {
	sequence += 1;
	const frame = {
		kind: "event" as const,
		connectionId: CONNECTION,
		generation: 1,
		sequence,
		sessionId,
		turnId,
		event,
	};
	acpStore.applyFrame(CONNECTION, frame);
	acpTranscript.applyFrame(frame);
}

/** Let the panel's connect-then-open-session chain settle. */
async function settle(): Promise<void> {
	for (let turn = 0; turn < 6; turn += 1) await Promise.resolve();
	await new Promise((resolve) => setTimeout(resolve, 0));
}

/** Open the panel and start a chat the way a person would, with "+". Nothing
 *  starts ego on its own any more (#1157-1e54). */
async function renderPanel() {
	const view = render(() => <AIChatPanel visible={true} repoPath={ROOT} onClose={() => {}} />);
	await settle();
	(view.container.querySelector('button[aria-label="New chat tab"]') as HTMLButtonElement).click();
	await settle();
	return view;
}

/** Type a message and press Send. */
async function typeAndSend(container: HTMLElement, text: string): Promise<void> {
	const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
	textarea.value = text;
	textarea.dispatchEvent(new Event("input", { bubbles: true }));
	(
		[...container.querySelectorAll("button")].find((button) => button.textContent === "Send") as HTMLButtonElement
	).click();
	await settle();
}

/** Open the panel without starting anything. */
function renderIdlePanel() {
	return render(() => <AIChatPanel visible={true} repoPath={ROOT} onClose={() => {}} />);
}

beforeEach(() => {
	vi.clearAllMocks();
	window.history.replaceState(null, "", "/");
	vi.mocked(invoke).mockImplementation(async (command) => {
		if (command === "load_config") return { ai_chat_sessions: {} };
		if (command === "get_home_directory") return HOME;
		return undefined;
	});
	sequence = 0;
	supportsImages = false;
	settings.egoExecutable = "/usr/local/bin/ego";
	acpStore.reset();
	acpTranscript.reset();
	resetAcpChatBindings();
	localStorage.clear();
	aiChatTabs.resetMemory();
	aiChatDraft.reset();

	client.connect.mockImplementation(async () => {
		const opened = snapshot();
		acpStore.applySnapshot(opened);
		acpStore.markStreaming(CONNECTION);
		return opened;
	});
	client.newSession.mockImplementation(async () => {
		acpStore.applySnapshot(snapshot({ attachments: [attachment()] }));
		return SESSION;
	});
	client.listSessions.mockResolvedValue({ sessions: [], nextCursor: null });
	// A replacement is a fresh process: nothing is attached until it loads.
	client.reconnect.mockImplementation(async () => {
		const opened = snapshot();
		acpStore.applySnapshot(opened);
		acpStore.markStreaming(CONNECTION);
		return opened;
	});
	for (const method of [
		"disconnect",
		"loadSession",
		"cancel",
		"cancelQueued",
		"answerPermission",
		"cancelPermission",
		"answerElicitation",
		"setConfigOption",
		"pause",
		"resumeTurn",
		"compact",
	] as const) {
		client[method].mockResolvedValue(undefined);
	}
	client.prompt.mockResolvedValue("turn-1");
});

afterEach(cleanup);

describe("AIChatPanel: the frame it keeps", () => {
	// The panel keeps its slot, its id and its detach control across the engine
	// swap. The registry entry behind this button is what makes Cmd+Alt+A, the
	// status-bar button and the command-palette entry work as well.
	it("offers its own window", async () => {
		const { container } = await renderPanel();
		await settle();

		expect(container.querySelector("#ai-chat-panel")).not.toBeNull();
		const detach = container.querySelector('button[title="Open in separate window"]') as HTMLButtonElement;
		detach.click();
		expect(mockDetachPanel).toHaveBeenCalledWith("ai-chat");
	});

	// The header names the repository, not the focused terminal: the panel binds
	// to a repo root and a session, and a per-terminal binding is the exact
	// inverse of a control plane.
	it("names the repository it is bound to", async () => {
		const { container } = await renderPanel();
		await settle();

		expect(container.textContent).toContain("tuicommander");
	});
});

describe("AIChatPanel: transcript actions", () => {
	it("reserves a copy row in both messages and keeps the tool count on one line", async () => {
		const style = document.createElement("style");
		style.textContent = readFileSync(
			resolve(process.cwd(), "src/components/AIChatPanel/AIChatPanel.module.css"),
			"utf8",
		);
		document.head.append(style);
		try {
			const { container } = await renderPanel();
			await settle();
			feed({ kind: "promptSent", text: "Question" });
			feed({
				kind: "sessionUpdate",
				update: {
					sessionUpdate: "agent_message_chunk",
					content: { type: "text", text: "Answer" },
				},
			});
			feed({
				kind: "sessionUpdate",
				update: {
					sessionUpdate: "tool_call",
					toolCallId: "one",
					title: "Inspect",
					status: "completed",
				},
			});
			await settle();
			for (const label of ["Copy user message", "Copy assistant message"]) {
				const button = container.querySelector(`button[aria-label="${label}"]`)!;
				expect(getComputedStyle(button).minHeight, label).toBe("18px");
			}
			const count = container.querySelector(".toolCallCount")!;
			expect(getComputedStyle(count).whiteSpace).toBe("nowrap");
		} finally {
			style.remove();
		}
	});
	it("follows new output at the bottom and keeps the reading position after scrolling up", async () => {
		const { container } = await renderPanel();
		await settle();
		const transcript = container.querySelector('[aria-label="Chat transcript"]') as HTMLDivElement;
		Object.defineProperties(transcript, {
			clientHeight: { configurable: true, value: 200 },
			scrollHeight: { configurable: true, value: 900 },
		});
		transcript.scrollTop = 700;
		transcript.dispatchEvent(new Event("scroll"));
		feed({
			kind: "sessionUpdate",
			update: {
				sessionUpdate: "agent_message_chunk",
				content: { type: "text", text: "First answer" },
			},
		});
		await settle();
		expect(transcript.scrollTop).toBe(900);
		transcript.scrollTop = 300;
		transcript.dispatchEvent(new Event("scroll"));
		feed({
			kind: "sessionUpdate",
			update: {
				sessionUpdate: "agent_message_chunk",
				content: { type: "text", text: " and more" },
			},
		});
		await settle();
		expect(transcript.scrollTop).toBe(300);
	});

	it("brings the typing indicator into view when a turn starts at the bottom", async () => {
		const { container } = await renderPanel();
		await settle();
		const transcript = container.querySelector('[aria-label="Chat transcript"]') as HTMLDivElement;
		Object.defineProperties(transcript, {
			clientHeight: { configurable: true, value: 200 },
			scrollHeight: { configurable: true, value: 900 },
		});
		transcript.scrollTop = 700;
		transcript.dispatchEvent(new Event("scroll"));
		feed({ kind: "turnStarted" }, SESSION, "turn-1");
		await settle();
		expect(container.querySelector(".thinkingPulse")).not.toBeNull();
		expect(transcript.scrollTop).toBe(900);
	});
	// Catches: a permanently visible Copy label or a button removed from keyboard focus.
	it("hides message Copy at rest while keeping it keyboard focusable", async () => {
		const style = document.createElement("style");
		style.textContent = readFileSync(
			resolve(process.cwd(), "src/components/AIChatPanel/AIChatPanel.module.css"),
			"utf8",
		);
		document.head.append(style);
		try {
			const { container } = await renderPanel();
			await settle();
			feed({ kind: "promptSent", text: "Question" });
			feed({
				kind: "sessionUpdate",
				update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "Answer" } },
			});
			await settle();
			for (const label of ["Copy user message", "Copy assistant message"]) {
				const button = container.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)!;
				expect(getComputedStyle(button).opacity, label).toBe("0");
				button.focus();
				expect(document.activeElement, label).toBe(button);
				button.blur();
			}
		} finally {
			style.remove();
		}
	});

	it("makes message text, tool output, and code selectable under the global no-selection rule", async () => {
		const style = document.createElement("style");
		style.textContent = ["src/global.css", "src/components/AIChatPanel/AIChatPanel.module.css"]
			.map((path) => readFileSync(resolve(process.cwd(), path), "utf8"))
			.join("\n");
		document.head.append(style);
		try {
			const { container } = await renderPanel();
			await settle();
			feed({ kind: "promptSent", text: "Question" });
			feed({
				kind: "sessionUpdate",
				update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "```sh\necho answer\n```" } },
			});
			feed({
				kind: "sessionUpdate",
				update: {
					sessionUpdate: "tool_call",
					toolCallId: "selectable-output",
					title: "Run",
					status: "completed",
					content: [{ type: "content", content: { type: "text", text: "tool output" } }],
				},
			});
			await settle();
			for (const selector of [".userMsg", ".assistantMsg pre", ".toolCallBody"]) {
				const element = container.querySelector(selector);
				expect(element, selector).not.toBeNull();
				expect(getComputedStyle(element!).userSelect, selector).toBe("text");
			}
		} finally {
			style.remove();
		}
	});
	it("sends a detached file link to the main-window terminal opener", async () => {
		window.history.replaceState(null, "", "/?mode=panel");
		vi.mocked(invoke).mockImplementation(async (command) => {
			if (command === "load_config") return { ai_chat_sessions: {} };
			if (command === "get_home_directory") return HOME;
			if (command === "resolve_terminal_path") return { absolute_path: "/repo/tuicommander/src/main.ts" };
			return undefined;
		});
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "sessionUpdate",
			update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "[file](src/main.ts)" } },
		});
		await settle();
		(container.querySelector(".assistantMsg a") as HTMLAnchorElement).click();
		await settle();
		expect(emitTo).toHaveBeenCalledWith("main", "panel-action", {
			panelId: "ai-chat",
			action: "open-file",
			data: { path: "/repo/tuicommander/src/main.ts" },
		});
		expect(mockOpenFile).not.toHaveBeenCalled();
		aiChatPanelAdapter.handleAction?.("open-file", { path: "/repo/tuicommander/src/main.ts" });
		expect(mockOpenFile).toHaveBeenCalledWith("/repo/tuicommander/src/main.ts");
	});
	it("copies the raw user message, assistant answer, and fenced code", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({ kind: "promptSent", text: "Question <one>" });
		feed({
			kind: "sessionUpdate",
			update: {
				sessionUpdate: "agent_message_chunk",
				content: { type: "text", text: "Answer **two**\n\n```sh\necho three\n```" },
			},
		});
		await settle();
		const buttons = [...container.querySelectorAll<HTMLButtonElement>('button[aria-label^="Copy "]')];
		for (const button of buttons) button.click();
		await settle();
		expect(mockWriteClipboard.mock.calls.map(([text]) => text)).toContain("Question <one>");
		expect(mockWriteClipboard.mock.calls.map(([text]) => text)).toContain("Answer **two**\n\n```sh\necho three\n```");
		expect(mockWriteClipboard.mock.calls.map(([text]) => text)).toContain("echo three");
	});

	it("opens web links externally and file links through the terminal file opener", async () => {
		vi.mocked(invoke).mockImplementation(async (command) => {
			if (command === "load_config") return { ai_chat_sessions: {} };
			if (command === "get_home_directory") return HOME;
			if (command === "resolve_terminal_path") return { absolute_path: "/repo/tuicommander/src/main.ts" };
			return undefined;
		});
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "sessionUpdate",
			update: {
				sessionUpdate: "agent_message_chunk",
				content: {
					type: "text",
					text: "[Website](https://example.com/help) and [source](/repo/tuicommander/src/main.ts)",
				},
			},
		});
		await settle();
		const links = [...container.querySelectorAll<HTMLAnchorElement>(".assistantMsg a")];
		links.forEach((link) => link.click());
		await settle();
		expect(mockOpenUrl).toHaveBeenCalledWith("https://example.com/help");
		expect(mockOpenFile).toHaveBeenCalledWith("/repo/tuicommander/src/main.ts");
	});

	it("makes a bare source path clickable only after the backend resolves it", async () => {
		vi.mocked(invoke).mockImplementation(async (command) => {
			if (command === "load_config") return { ai_chat_sessions: {} };
			if (command === "get_home_directory") return HOME;
			if (command === "resolve_terminal_path") return { absolute_path: "/repo/tuicommander/src/main.ts" };
			return undefined;
		});
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "sessionUpdate",
			update: {
				sessionUpdate: "agent_message_chunk",
				content: { type: "text", text: "Open src/main.ts:42 to inspect it." },
			},
		});
		await settle();
		const link = [...container.querySelectorAll<HTMLAnchorElement>(".assistantMsg a")].find(
			(anchor) => anchor.textContent === "src/main.ts:42",
		);
		expect(link).toBeDefined();
		link?.click();
		await settle();
		// Relative to where ego runs, which is the workspace, not the viewed repo.
		expect(invoke).toHaveBeenCalledWith("resolve_terminal_path", { cwd: CHAT_ROOT, candidate: "src/main.ts:42" });
		expect(mockOpenFile).toHaveBeenCalledWith("/repo/tuicommander/src/main.ts", undefined, 42, undefined);
	});

	it("opens links in a user message through the same URL and file handlers", async () => {
		vi.mocked(invoke).mockImplementation(async (command) => {
			if (command === "load_config") return { ai_chat_sessions: {} };
			if (command === "get_home_directory") return HOME;
			if (command === "resolve_terminal_path") return { absolute_path: "/repo/tuicommander/src/main.ts" };
			return undefined;
		});
		const { container } = await renderPanel();
		await settle();
		feed({ kind: "promptSent", text: "Open https://example.com/help and src/main.ts" });
		await settle();
		const links = [...container.querySelectorAll<HTMLAnchorElement>(".userMsg a")];
		expect(links.map((link) => link.textContent)).toEqual(["https://example.com/help", "src/main.ts"]);
		links.forEach((link) => link.click());
		await settle();
		expect(mockOpenUrl).toHaveBeenCalledWith("https://example.com/help");
		expect(mockOpenFile).toHaveBeenCalledWith("/repo/tuicommander/src/main.ts");
	});

	it("keeps a failed file lookup inside the panel without opening a path", async () => {
		vi.mocked(invoke).mockImplementation(async (command) => {
			if (command === "load_config") return { ai_chat_sessions: {} };
			if (command === "get_home_directory") return HOME;
			if (command === "resolve_terminal_path") throw new Error("resolver unavailable");
			return undefined;
		});
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "sessionUpdate",
			update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "[file](src/main.ts)" } },
		});
		await settle();
		(container.querySelector(".assistantMsg a") as HTMLAnchorElement).click();
		await settle();
		expect(mockOpenFile).not.toHaveBeenCalled();
		expect(container.textContent).toContain("file");
	});
});

describe("AIChatPanel: one chat across repositories", () => {
	function renderSwitchable() {
		const [repo, setRepo] = createSignal<string | null>(ROOT);
		const view = render(() => <AIChatPanel visible={true} repoPath={repo()} onClose={() => {}} />);
		return { ...view, setRepo };
	}

	async function send(container: HTMLElement, text: string): Promise<void> {
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		textarea.value = text;
		textarea.dispatchEvent(new Event("input", { bubbles: true }));
		(
			[...container.querySelectorAll("button")].find((button) => button.textContent === "Send") as HTMLButtonElement
		).click();
		await settle();
	}

	it("starts nothing when the panel opens or the repository changes", async () => {
		const { setRepo } = renderSwitchable();
		await settle();
		setRepo("/repo/other");
		await settle();
		setRepo(null);
		await settle();
		expect(client.connect).not.toHaveBeenCalled();
		expect(client.newSession).not.toHaveBeenCalled();
		expect(client.loadSession).not.toHaveBeenCalled();
	});

	it("starts one ego on the first message, in the workspace root, and a second tab reuses it", async () => {
		client.newSession.mockResolvedValueOnce(SESSION).mockResolvedValueOnce(SECOND_SESSION);
		const { container } = renderSwitchable();
		await settle();
		await send(container, "hello");
		expect(client.connect).toHaveBeenCalledTimes(1);
		expect(client.connect).toHaveBeenCalledWith(CHAT_ROOT);
		expect(client.newSession).toHaveBeenCalledWith(CONNECTION, CHAT_ROOT);
		expect(client.prompt).toHaveBeenCalledTimes(1);
		(container.querySelector('button[aria-label="New chat tab"]') as HTMLButtonElement).click();
		await settle();
		expect(client.newSession).toHaveBeenCalledTimes(2);
		expect(client.newSession).toHaveBeenLastCalledWith(CONNECTION, CHAT_ROOT);
		expect(client.connect).toHaveBeenCalledTimes(1);
	});

	it("keeps the same tabs and the same chat when the repository changes", async () => {
		client.newSession.mockResolvedValueOnce(SESSION).mockResolvedValueOnce(SECOND_SESSION);
		const { container, setRepo } = renderSwitchable();
		await settle();
		await send(container, "hello");
		(container.querySelector('button[aria-label="New chat tab"]') as HTMLButtonElement).click();
		await settle();
		const tabs = () =>
			[...container.querySelectorAll("[data-chat-session]")].map((tab) => tab.getAttribute("data-chat-session"));
		const before = tabs();
		expect(before).toEqual([SESSION, SECOND_SESSION]);
		setRepo("/repo/other");
		await settle();
		expect(tabs()).toEqual(before);
		await send(container, "still here");
		expect(client.prompt).toHaveBeenLastCalledWith(CONNECTION, SECOND_SESSION, "still here", [], "/repo/other");
		expect(client.connect).toHaveBeenCalledTimes(1);
		expect(client.loadSession).not.toHaveBeenCalled();
	});

	// A reloaded document connects again and is handed the ego that is already
	// running, with its sessions still attached; loading one again is refused.
	it("does not replay a conversation the running ego already has attached", async () => {
		vi.mocked(invoke).mockImplementation(async (command) => {
			if (command === "load_config") return { ai_chat_sessions: { [CHAT_ROOT]: SESSION } };
			if (command === "get_home_directory") return HOME;
			return undefined;
		});
		client.connect.mockImplementation(async () => {
			const adopted = snapshot({ attachments: [attachment()] });
			acpStore.applySnapshot(adopted);
			acpStore.markStreaming(CONNECTION);
			return adopted;
		});
		const { container } = renderSwitchable();
		await settle();
		await send(container, "carry on");
		expect(client.loadSession).not.toHaveBeenCalled();
		expect(client.newSession).not.toHaveBeenCalled();
		expect(client.prompt).toHaveBeenLastCalledWith(CONNECTION, SESSION, "carry on", [], ROOT);
	});

	it("sends the viewed repository with each prompt as context, never as the session's cwd", async () => {
		const { container, setRepo } = renderSwitchable();
		await settle();
		await send(container, "what is here");
		expect(client.prompt).toHaveBeenLastCalledWith(CONNECTION, SESSION, "what is here", [], ROOT);
		setRepo(null);
		await settle();
		await send(container, "and now");
		expect(client.prompt).toHaveBeenLastCalledWith(CONNECTION, SESSION, "and now", [], null);
		expect(client.newSession).toHaveBeenCalledWith(CONNECTION, CHAT_ROOT);
		expect(client.newSession).not.toHaveBeenCalledWith(CONNECTION, ROOT);
	});
});

describe("AIChatPanel: parallel tabs", () => {
	it("keeps both tabs and transcripts when the panel is hidden and shown", async () => {
		client.newSession.mockResolvedValueOnce(SESSION).mockResolvedValueOnce(SECOND_SESSION);
		const [visible, setVisible] = createSignal(true);
		const { container } = render(() => <AIChatPanel visible={visible()} repoPath={ROOT} onClose={() => {}} />);
		await settle();
		for (const _ of [SESSION, SECOND_SESSION]) {
			(container.querySelector('button[aria-label="New chat tab"]') as HTMLButtonElement).click();
			await settle();
		}
		feed(
			{
				kind: "sessionUpdate",
				update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "Background reply" } },
			},
			SESSION,
		);
		setVisible(false);
		setVisible(true);
		(container.querySelector(`button[data-chat-session="${SESSION}"]`) as HTMLButtonElement).click();
		await settle();
		expect(container.querySelectorAll("[data-chat-session]")).toHaveLength(2);
		expect(container.textContent).toContain("Background reply");
		expect(client.connect).toHaveBeenCalledTimes(1);
		expect(client.disconnect).not.toHaveBeenCalled();
	});
	/** Two tabs on a live connection where neither has an attachment yet, then
	 *  hidden and shown so the bound-root path replays them. */
	async function unattachedTabs(visible: () => boolean, setVisible: (value: boolean) => void) {
		client.newSession.mockResolvedValueOnce(SESSION).mockResolvedValueOnce(SECOND_SESSION);
		const view = render(() => <AIChatPanel visible={visible()} repoPath={ROOT} onClose={() => {}} />);
		await settle();
		for (const _ of [SESSION, SECOND_SESSION]) {
			(view.container.querySelector('button[aria-label="New chat tab"]') as HTMLButtonElement).click();
			await settle();
		}
		setVisible(false);
		setVisible(true);
		await settle();
		return view;
	}
	const loadsOf = (session: string) => client.loadSession.mock.calls.filter((call) => call[1] === session).length;

	it("does not load a tab again while its first load is still pending", async () => {
		client.loadSession.mockImplementation(() => new Promise(() => {}));
		const [visible, setVisible] = createSignal(true);
		await unattachedTabs(visible, setVisible);
		expect(loadsOf(SECOND_SESSION)).toBe(1);
		acpStore.applySnapshot(snapshot());
		feed({
			kind: "sessionUpdate",
			update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "x" } },
		});
		setVisible(false);
		setVisible(true);
		await settle();
		expect(loadsOf(SECOND_SESSION)).toBe(1);
	});

	it("does not re-load a failed tab on the next update, and says why", async () => {
		client.loadSession.mockImplementation(async (_id, session) => {
			if (session === SECOND_SESSION) throw { kind: "agent", message: "MCP admission refused" };
		});
		const [visible, setVisible] = createSignal(true);
		const { container } = await unattachedTabs(visible, setVisible);
		expect(loadsOf(SECOND_SESSION)).toBe(1);
		expect(container.textContent).toContain("MCP admission refused");
		acpStore.applySnapshot(snapshot());
		setVisible(false);
		setVisible(true);
		await settle();
		expect(loadsOf(SECOND_SESSION)).toBe(1);
		(container.querySelector(`button[data-chat-session="${SESSION}"]`) as HTMLButtonElement).click();
		await settle();
		(container.querySelector(`button[data-chat-session="${SECOND_SESSION}"]`) as HTMLButtonElement).click();
		await settle();
		expect(loadsOf(SECOND_SESSION)).toBe(2);
	});

	it("keeps a terminal context-menu draft queued before the first session opens", async () => {
		aiChatDraft.append("Explain this selected error");
		const { container } = await renderPanel();
		await settle();
		expect((container.querySelector("textarea") as HTMLTextAreaElement).value).toBe("Explain this selected error");
	});
	it("routes prompts to the selected session and returns to the neighbor when closing it", async () => {
		client.newSession.mockResolvedValueOnce(SESSION).mockResolvedValueOnce(SECOND_SESSION);
		const { container } = await renderPanel();
		await settle();
		(container.querySelector('button[aria-label="New chat tab"]') as HTMLButtonElement).click();
		await settle();
		let textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		textarea.value = "Second request";
		textarea.dispatchEvent(new Event("input", { bubbles: true }));
		(
			[...container.querySelectorAll("button")].find((button) => button.textContent === "Send") as HTMLButtonElement
		).click();
		await settle();
		expect(client.prompt).toHaveBeenCalledWith(CONNECTION, SECOND_SESSION, "Second request", [], ROOT);
		(container.querySelector(`button[aria-label="Close chat tab ${SECOND_SESSION}"]`) as HTMLButtonElement).click();
		await settle();
		textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		textarea.value = "First request";
		textarea.dispatchEvent(new Event("input", { bubbles: true }));
		(
			[...container.querySelectorAll("button")].find((button) => button.textContent === "Send") as HTMLButtonElement
		).click();
		await settle();
		expect(client.prompt).toHaveBeenCalledWith(CONNECTION, SESSION, "First request", [], ROOT);
		expect(client.disconnect).not.toHaveBeenCalled();
	});
	it("opens a new tab with the focused-panel shortcut", async () => {
		client.newSession.mockResolvedValueOnce(SESSION).mockResolvedValueOnce(SECOND_SESSION);
		const { container } = await renderPanel();
		await settle();
		(container.querySelector("textarea") as HTMLTextAreaElement).dispatchEvent(
			new KeyboardEvent("keydown", { key: "t", metaKey: true, bubbles: true }),
		);
		await settle();
		expect(container.querySelectorAll("[data-chat-session]")).toHaveLength(2);
	});

	it("keeps separate ACP transcripts and composer drafts while switching and closing tabs", async () => {
		client.newSession.mockResolvedValueOnce(SESSION).mockResolvedValueOnce(SECOND_SESSION);
		const { container } = await renderPanel();
		await settle();
		feed(
			{
				kind: "sessionUpdate",
				update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "First answer" } },
			},
			SESSION,
		);
		await settle();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		textarea.value = "Draft for first";
		textarea.dispatchEvent(new Event("input", { bubbles: true }));
		(container.querySelector('button[aria-label="New chat tab"]') as HTMLButtonElement).click();
		await settle();
		feed(
			{
				kind: "sessionUpdate",
				update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "Second answer" } },
			},
			SECOND_SESSION,
		);
		await settle();
		expect(container.textContent).toContain("Second answer");
		expect(container.textContent).not.toContain("First answer");
		const secondTextarea = container.querySelector("textarea") as HTMLTextAreaElement;
		expect(secondTextarea.value).toBe("");
		secondTextarea.value = "Draft for second";
		secondTextarea.dispatchEvent(new Event("input", { bubbles: true }));
		(container.querySelector(`button[data-chat-session="${SESSION}"]`) as HTMLButtonElement).click();
		await settle();
		expect(container.textContent).toContain("First answer");
		expect(container.textContent).not.toContain("Second answer");
		expect((container.querySelector("textarea") as HTMLTextAreaElement).value).toBe("Draft for first");
		(container.querySelector(`button[aria-label="Close chat tab ${SECOND_SESSION}"]`) as HTMLButtonElement).click();
		await settle();
		expect(container.querySelector(`button[data-chat-session="${SECOND_SESSION}"]`)).toBeNull();
		expect(container.textContent).toContain("First answer");
		expect(client.disconnect).not.toHaveBeenCalled();
	});

	it("restores both tabs and their transcripts after the panel mounts in a new document", async () => {
		client.newSession.mockResolvedValueOnce(SESSION).mockResolvedValueOnce(SECOND_SESSION);
		const first = await renderPanel();
		await settle();
		(first.container.querySelector('button[aria-label="New chat tab"]') as HTMLButtonElement).click();
		await settle();
		first.unmount();
		resetAcpChatBindings();
		aiChatTabs.resetMemory();
		acpStore.reset();
		acpTranscript.reset();
		client.loadSession.mockImplementation(async (_id, session) => {
			feed(
				{
					kind: "sessionUpdate",
					update: {
						sessionUpdate: "agent_message_chunk",
						content: { type: "text", text: session === SESSION ? "First replay" : "Second replay" },
					},
				},
				session,
			);
		});
		// The tabs come back with the document; their history comes back when
		// ego does, which is the next message.
		const second = renderIdlePanel();
		await settle();
		expect(second.container.querySelectorAll("[data-chat-session]")).toHaveLength(2);
		expect(client.loadSession).not.toHaveBeenCalled();
		const textarea = second.container.querySelector("textarea") as HTMLTextAreaElement;
		textarea.value = "where were we";
		textarea.dispatchEvent(new Event("input", { bubbles: true }));
		(
			[...second.container.querySelectorAll("button")].find(
				(button) => button.textContent === "Send",
			) as HTMLButtonElement
		).click();
		await settle();
		expect(second.container.textContent).toContain("Second replay");
		(second.container.querySelector(`button[data-chat-session="${SESSION}"]`) as HTMLButtonElement).click();
		await settle();
		expect(second.container.textContent).toContain("First replay");
	});
});

describe("AIChatPanel: transcript keyboard", () => {
	it("selects only the transcript, finds a term, and clears this tab's view", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({ kind: "promptSent", text: "First question" });
		feed({
			kind: "sessionUpdate",
			update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "Second answer" } },
		});
		await settle();
		const transcript = container.querySelector('[aria-label="Chat transcript"]') as HTMLDivElement;
		transcript.dispatchEvent(new KeyboardEvent("keydown", { key: "a", metaKey: true, bubbles: true }));
		expect(window.getSelection()?.toString()).toContain("First question");
		expect(window.getSelection()?.toString()).not.toContain("Ask ego about this repository");
		transcript.dispatchEvent(new KeyboardEvent("keydown", { key: "c", metaKey: true, bubbles: true }));
		expect(mockWriteClipboard.mock.calls.at(-1)?.[0]).toContain("First question");
		transcript.dispatchEvent(new KeyboardEvent("keydown", { key: "f", metaKey: true, bubbles: true }));
		const search = container.querySelector('input[aria-label="Find in chat"]') as HTMLInputElement;
		expect(search).not.toBeNull();
		search.value = "Second";
		search.dispatchEvent(new Event("input", { bubbles: true }));
		search.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
		expect(window.getSelection()?.toString()).toBe("Second");
		transcript.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true }));
		await settle();
		expect(container.textContent).not.toContain("Second answer");
	});
});

describe("AIChatPanel: without a configured binary", () => {
	// An empty `ego_executable` is refused in Rust at connect. Saying so is the
	// difference between a panel that explains itself and one that silently
	// never starts.
	it("explains that ACP is not configured and launches nothing", async () => {
		settings.egoExecutable = "";
		const { container } = renderIdlePanel();
		await settle();

		expect(container.textContent).toContain("ACP is not configured");
		expect(client.connect).not.toHaveBeenCalled();
		expect(container.querySelector("textarea")).toBeNull();
	});
});

describe("AIChatPanel: a turn", () => {
	it("removes the complete first-turn ack after ego streams it in tiny chunks", async () => {
		const { container } = await renderPanel();
		await settle();
		// Recorded AssistantDelta text from ego session 2080b5ad, events 13-47.
		const chunks = [
			"T", "UI", "Commander", " v", "1", ".", "7", ".", "7", " is", " connected", ".\n",
			"intent", ":", " Ver", "ifico", " gli", " agent", "i", " att", "ivi", " e", " ti", " ri",
			"porto", " lo", " stato", " att", "uale", " (", "Ag", "enti", " att", "ivi", ")",
		];
		for (const chunk of chunks) {
			feed({
				kind: "sessionUpdate",
				update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: chunk } },
			});
			await settle();
		}
		const intent = container.querySelector('[aria-label="Agent intent"]');
		expect(intent?.textContent).toContain("Verifico gli agenti attivi e ti riporto lo stato attuale");
		expect(container.textContent).not.toContain(".7.7 is connected.");
	});

	it("keeps a long paste compact in the composer but sends every original word", async () => {
		const { container } = await renderPanel();
		await settle();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		const pasted = Array.from({ length: 201 }, (_, index) => `word${index}`).join(" ");
		const event = new Event("paste", { bubbles: true, cancelable: true });
		Object.defineProperty(event, "clipboardData", { value: { items: [], getData: () => pasted } });
		textarea.dispatchEvent(event);
		await settle();
		expect(event.defaultPrevented).toBe(true);
		expect(textarea.value).toBe("[Pasted text #1 +201 words]");
		(container.querySelector("button.sendBtn") as HTMLButtonElement).click();
		await settle();
		expect(client.prompt).toHaveBeenCalledWith(CONNECTION, SESSION, pasted, [], ROOT);
	});

	it("preserves ordinary paste at the 200-word boundary", async () => {
		const { container } = await renderPanel();
		await settle();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		const event = new Event("paste", { bubbles: true, cancelable: true });
		Object.defineProperty(event, "clipboardData", {
			value: { items: [], getData: () => Array(200).fill("word").join(" ") },
		});
		textarea.dispatchEvent(event);
		expect(event.defaultPrevented).toBe(false);
	});

	it("keeps typed text around a large paste in the sent prompt", async () => {
		const { container } = await renderPanel();
		await settle();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		textarea.value = "Before after";
		textarea.dispatchEvent(new Event("input", { bubbles: true }));
		textarea.setSelectionRange(7, 7);
		const pasted = Array(201).fill("detail").join(" ");
		const event = new Event("paste", { bubbles: true, cancelable: true });
		Object.defineProperty(event, "clipboardData", { value: { items: [], getData: () => pasted } });
		textarea.dispatchEvent(event);
		await settle();
		expect(textarea.value).toBe("Before [Pasted text #1 +201 words]after");
		(container.querySelector("button.sendBtn") as HTMLButtonElement).click();
		await settle();
		expect(client.prompt).toHaveBeenCalledWith(CONNECTION, SESSION, `Before ${pasted}after`, [], ROOT);
	});

	it("grows the composer with input until its maximum height", async () => {
		const { container } = await renderPanel();
		await settle();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		Object.defineProperty(textarea, "scrollHeight", { configurable: true, value: 108 });
		textarea.value = "several lines\nmore lines";
		textarea.dispatchEvent(new Event("input", { bubbles: true }));
		expect(textarea.style.height).toBe("108px");
		Object.defineProperty(textarea, "scrollHeight", { configurable: true, value: 600 });
		textarea.dispatchEvent(new Event("input", { bubbles: true }));
		expect(textarea.style.height).toBe("150px");
	});
	it("hides the connection ack and presents intent as turn status", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "sessionUpdate",
			update: {
				sessionUpdate: "agent_message_chunk",
				content: {
					type: "text",
					text: "TUICommander v1.7.7 is connected. intent: Controllo lo stato prima di risponderti (Stato)\nRisultato pronto.",
				},
			},
		});
		await settle();
		expect(container.textContent).not.toContain("TUICommander v1.7.7 is connected.");
		expect(container.textContent).not.toContain("intent:");
		expect(container.querySelector('[aria-label="Agent intent"]')?.textContent).toContain(
			"Controllo lo stato prima di risponderti",
		);
		expect(container.textContent).toContain("Risultato pronto.");
	});

	it("turns a streamed suggestion into three actions and submits the chosen text", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "sessionUpdate",
			update: {
				sessionUpdate: "agent_message_chunk",
				content: { type: "text", text: "Scegli il prossimo passo.\nsug" },
			},
		});
		feed({
			kind: "sessionUpdate",
			update: {
				sessionUpdate: "agent_message_chunk",
				content: { type: "text", text: "gest: [ Stato lavori | Una decisione aperta | Nuova richiesta ]" },
			},
		});
		await settle();
		expect(container.textContent).not.toContain("suggest:");
		const choices = [...container.querySelectorAll('[aria-label="Suggested replies"] button')];
		expect(choices.map((button) => button.textContent)).toEqual([
			"Stato lavori",
			"Una decisione aperta",
			"Nuova richiesta",
		]);
		(choices[1] as HTMLButtonElement).click();
		await settle();
		expect(client.prompt).toHaveBeenCalledWith(CONNECTION, SESSION, "Una decisione aperta", [], ROOT);
		feed({ kind: "promptSent", text: "Una decisione aperta" });
		feed({
			kind: "sessionUpdate",
			update: { sessionUpdate: "user_message_chunk", content: { type: "text", text: "Una decisione aperta" } },
		});
		await settle();
		// Catches: the suggestion click creates a bubble that doubles when ego echoes the prompt.
		expect(
			[...container.querySelectorAll(".userMsg")].map((message) => message.textContent?.replace("Copy", "")),
		).toEqual(["Una decisione aperta"]);
	});

	// Catches: an inline token at the end of the answer remaining visible as raw text.
	it("turns a trailing inline suggestion into reply actions", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "sessionUpdate",
			update: {
				sessionUpdate: "agent_message_chunk",
				content: { type: "text", text: "The checks are active. suggest: [ Retry | Show status | Diagnose ]" },
			},
		});
		await settle();
		expect(container.querySelector(".assistantMsg")?.textContent).toContain("The checks are active.");
		expect(container.querySelector(".assistantMsg")?.textContent).not.toContain("suggest:");
		expect(
			[...container.querySelectorAll('[aria-label="Suggested replies"] button')].map((button) => button.textContent),
		).toEqual(["Retry", "Show status", "Diagnose"]);
	});

	// Catches: parsing a protocol-looking phrase before the end of the answer.
	it("keeps an inline suggestion before further prose as answer text", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "sessionUpdate",
			update: {
				sessionUpdate: "agent_message_chunk",
				content: { type: "text", text: "The syntax is suggest: [ A | B ] in this example.\nMore explanation follows." },
			},
		});
		await settle();
		expect(container.querySelector(".assistantMsg")?.textContent).toContain("suggest: [ A | B ] in this example.");
		expect(container.querySelector('[aria-label="Suggested replies"]')).toBeNull();
	});

	it("leaves mentions of protocol words inside prose and fenced code unchanged", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "sessionUpdate",
			update: {
				sessionUpdate: "agent_message_chunk",
				content: {
					type: "text",
					text: "I suggest: [ A | B ] in the sample.\n```text\nsuggest: [ Alpha | Beta ]\nintent: sample (Demo)\n```",
				},
			},
		});
		await settle();
		expect(container.textContent).toContain("I suggest: [ A | B ]");
		expect(container.textContent).toContain("suggest: [ Alpha | Beta ]");
		expect(container.textContent).toContain("intent: sample (Demo)");
		expect(container.querySelector('[aria-label="Suggested replies"]')).toBeNull();
	});

	it("keeps malformed suggestions and mid-sentence intent as answer text", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "sessionUpdate",
			update: {
				sessionUpdate: "agent_message_chunk",
				content: {
					type: "text",
					text: "The intent: of this example is explanatory.\nsuggest: [ A | B\nsuggest: [ A | nested [ B ] | C ]\nsuggest: [ A | B | C | D | E ]",
				},
			},
		});
		await settle();
		expect(container.textContent).toContain("The intent: of this example");
		expect(container.textContent).toContain("suggest: [ A | B");
		expect(container.textContent).toContain("suggest: [ A | nested [ B ] | C ]");
		expect(container.textContent).toContain("suggest: [ A | B | C | D | E ]");
		expect(container.querySelector('[aria-label="Agent intent"]')).toBeNull();
		expect(container.querySelector('[aria-label="Suggested replies"]')).toBeNull();
	});

	it("does not interpret markers in indented markdown code", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "sessionUpdate",
			update: {
				sessionUpdate: "agent_message_chunk",
				content: { type: "text", text: "Example:\n\n    suggest: [ Yes | No ]\n    intent: show syntax (Example)" },
			},
		});
		await settle();
		expect(container.textContent).toContain("suggest: [ Yes | No ]");
		expect(container.textContent).toContain("intent: show syntax (Example)");
		expect(container.querySelector('[aria-label="Suggested replies"]')).toBeNull();
	});

	it("keeps an indented code example at the start of an answer", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "sessionUpdate",
			update: {
				sessionUpdate: "agent_message_chunk",
				content: { type: "text", text: "    suggest: [ A | B ]" },
			},
		});
		await settle();
		expect(container.querySelector("pre code")?.textContent).toContain("suggest: [ A | B ]");
		expect(container.querySelector('[aria-label="Suggested replies"]')).toBeNull();
	});

	it("shows an ACP prompt failure in the transcript and returns the composer to Send", async () => {
		const { container } = await renderPanel();
		await settle();
		acpStore.applySnapshot(
			snapshot({
				attachments: [
					attachment({
						state: "prompting",
						activeTurn: { turnId: "turn-1", state: "running", stopReason: null, usage: null },
					}),
				],
			}),
		);
		feed(
			{ kind: "turnFailed", message: "no capabilities are configured for `openai-codex/gpt-5.6-sol`", state: "idle" },
			SESSION,
			"turn-1",
		);
		await settle();
		expect(container.textContent).toContain("no capabilities are configured");
		expect([...container.querySelectorAll("button")].some((button) => button.textContent === "Send")).toBe(true);
	});

	it("shows that a normally settled turn without an agent reply ended", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({ kind: "promptSent", text: "ciao" });
		feed({ kind: "turnSettled", stopReason: "end_turn", usage: null });
		await settle();
		expect(container.textContent).toContain("Turn ended without a reply");
	});

	it("shows an agent refusal after the prompt settles", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({ kind: "promptSent", text: "ciao" });
		feed({ kind: "turnSettled", stopReason: "refusal", usage: null });
		await settle();
		expect(container.textContent).toContain("The agent refused this turn.");
	});

	it("shows the shared queue, can remove the phone prompt, and accepts a desktop prompt while busy", async () => {
		const { container } = await renderPanel();
		await settle();
		acpStore.applySnapshot(
			snapshot({
				attachments: [
					attachment({
						state: "prompting",
						activeTurn: { turnId: "running", state: "running", stopReason: null, usage: null },
						queuedPrompts: [{ turnId: "phone-queued", summary: "from phone" }],
					}),
				],
			}),
		);
		await settle();
		expect(container.textContent).toContain("from phone");
		const remove = container.querySelector('button[aria-label="Cancel queued prompt from phone"]') as HTMLButtonElement;
		expect(remove).not.toBeNull();
		remove.click();
		await settle();
		expect(client.cancelQueued).toHaveBeenCalledWith(CONNECTION, SESSION, "phone-queued");

		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		textarea.value = "desktop next";
		textarea.dispatchEvent(new Event("input", { bubbles: true }));
		await settle();
		[...container.querySelectorAll("button")].find((button) => button.textContent === "Queue")?.click();
		await settle();
		expect(client.prompt).toHaveBeenCalledWith(CONNECTION, SESSION, "desktop next", [], ROOT);
	});
	// Catches: an image paste is ignored even though ego advertises image prompts.
	it("stages a pasted PNG and sends it with the next turn", async () => {
		supportsImages = true;
		const { container } = await renderPanel();
		await settle();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		const file = new File([Uint8Array.from(atob(PNG_1X1), (byte) => byte.charCodeAt(0))], "clip.png", {
			type: "image/png",
		});
		const event = pasteFile(textarea, file);
		await vi.waitFor(() => expect(container.querySelector('img[alt="Pasted image"]')).not.toBeNull());

		expect(event.defaultPrevented).toBe(true);
		[...container.querySelectorAll("button")].find((button) => button.textContent === "Send")?.click();
		await settle();
		expect(client.prompt).toHaveBeenCalledWith(
			CONNECTION,
			SESSION,
			"",
			[{ type: "image", mimeType: "image/png", data: PNG_1X1 }],
			ROOT,
		);
	});

	// Catches: a paste is accepted for an agent that cannot read image blocks.
	it("refuses an image when the connection did not advertise image prompts", async () => {
		const { container } = await renderPanel();
		await settle();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		pasteFile(textarea, new File(["png"], "clip.png", { type: "image/png" }));
		await settle();

		expect(container.textContent).toContain("does not support images");
		expect(container.querySelector('img[alt="Pasted image"]')).toBeNull();
		expect(client.prompt).not.toHaveBeenCalled();
	});

	// Catches: a large clipboard blob is encoded before any size check.
	it("refuses an oversized image and reports its size", async () => {
		supportsImages = true;
		const { container } = await renderPanel();
		await settle();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		const read = vi.spyOn(FileReader.prototype, "readAsDataURL");
		pasteFile(textarea, new File([new Uint8Array(10 * 1024 * 1024 + 1)], "huge.png", { type: "image/png" }));
		await settle();

		expect(container.textContent).toContain("10 MiB");
		expect(container.querySelector('img[alt="Pasted image"]')).toBeNull();
		expect(client.prompt).not.toHaveBeenCalled();
		expect(read).not.toHaveBeenCalled();
		read.mockRestore();
	});

	// Catches: two quick paste events each pass the cap while the first read is pending.
	it("keeps rapid image pastes within the total size cap", async () => {
		supportsImages = true;
		const { container } = await renderPanel();
		await settle();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		pasteFile(textarea, new File([new Uint8Array(6 * 1024 * 1024)], "first.png", { type: "image/png" }));
		pasteFile(textarea, new File([new Uint8Array(6 * 1024 * 1024)], "second.png", { type: "image/png" }));

		await vi.waitFor(() => expect(container.textContent).toContain("total limit"));
		expect(container.querySelectorAll('img[alt="Pasted image"]')).toHaveLength(1);
	});

	// Catches: a staged image still goes on the wire after the person removes it.
	it("removes a staged image before sending the text", async () => {
		supportsImages = true;
		const { container } = await renderPanel();
		await settle();
		pasteFile(
			container.querySelector("textarea") as HTMLTextAreaElement,
			new File(["png"], "clip.png", { type: "image/png" }),
		);
		await vi.waitFor(() => expect(container.querySelector('button[aria-label="Remove pasted image"]')).not.toBeNull());
		(container.querySelector('button[aria-label="Remove pasted image"]') as HTMLButtonElement).click();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		textarea.value = "text only";
		textarea.dispatchEvent(new Event("input", { bubbles: true }));
		await settle();
		[...container.querySelectorAll("button")].find((button) => button.textContent === "Send")?.click();
		await settle();
		expect(client.prompt).toHaveBeenCalledWith(CONNECTION, SESSION, "text only", [], ROOT);
	});

	// Catches: intercepting all paste events breaks the browser's text insertion.
	it("leaves plain text paste to the textarea", async () => {
		const { container } = await renderPanel();
		await settle();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		const event = new Event("paste", { bubbles: true, cancelable: true });
		Object.defineProperty(event, "clipboardData", { value: { items: [], getData: () => "ordinary text" } });
		textarea.dispatchEvent(event);
		expect(event.defaultPrevented).toBe(false);
	});
	it("opens the connection and its session on the workspace root", async () => {
		await renderPanel();
		await settle();

		expect(client.connect).toHaveBeenCalledWith(CHAT_ROOT);
		expect(client.newSession).toHaveBeenCalledWith(CONNECTION, CHAT_ROOT);
	});

	it("sends what was typed and streams the answer back", async () => {
		const { container } = await renderPanel();
		await settle();

		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		textarea.value = "what does acp/mod.rs do?";
		textarea.dispatchEvent(new Event("input", { bubbles: true }));
		await settle();

		const send = [...container.querySelectorAll("button")].find((button) => button.textContent === "Send");
		send?.click();
		await settle();

		expect(client.prompt).toHaveBeenCalledWith(CONNECTION, SESSION, "what does acp/mod.rs do?", [], ROOT);

		feed({
			kind: "sessionUpdate",
			update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "It " } },
		});
		feed({
			kind: "sessionUpdate",
			update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "serializes." } },
		});
		await settle();

		expect(container.textContent).toContain("It serializes.");
	});

	// One connection per root. Coming back to a repository must not launch a
	// second ego on a root that already has one.
	it("reuses the connection a root already has", async () => {
		const first = await renderPanel();
		await settle();
		first.unmount();

		await renderPanel();
		await settle();

		expect(client.connect).toHaveBeenCalledTimes(1);
		expect(client.newSession).toHaveBeenCalledTimes(1);
	});
});

describe("AIChatPanel: durable conversations", () => {
	it("labels untitled conversations without exposing an id as the option text", async () => {
		client.listSessions.mockResolvedValue({
			sessions: [
				{ sessionId: SESSION, cwd: CHAT_ROOT, title: null, updatedAt: "2026-09-27T09:00:00Z" },
				{
					sessionId: "01932d5e-0000-7000-8000-0000000000bb",
					cwd: CHAT_ROOT,
					title: null,
					updatedAt: "2026-09-26T09:00:00Z",
				},
			],
			nextCursor: null,
		});
		const { container } = await renderPanel();
		await settle();
		const options = [...container.querySelectorAll('select[title="Conversation"] option')];
		expect(options).toHaveLength(2);
		expect(options[0].textContent).toContain("Conversation");
		expect(options[0].textContent).not.toContain(SESSION);
		expect(options[0].getAttribute("title")).toBe(SESSION);
	});

	it("shows a new ACP session title in the header and conversation picker", async () => {
		client.listSessions.mockResolvedValue({
			sessions: [
				{ sessionId: SESSION, cwd: CHAT_ROOT, title: "Conversation 1", updatedAt: "2026-09-27T09:00:00Z" },
				{ sessionId: "other", cwd: CHAT_ROOT, title: "Other", updatedAt: "2026-09-26T09:00:00Z" },
			],
			nextCursor: null,
		});
		const { container } = await renderPanel();
		await settle();
		feed({ kind: "sessionUpdate", update: { sessionUpdate: "session_info_update", title: "Review architecture" } });
		await settle();

		expect(container.querySelector('select[title="Conversation"]')?.textContent).toContain("Review architecture");
		expect(container.querySelector('[class*="headerLeft"]')?.textContent).toContain("Review architecture");
	});

	it("renders context occupancy and cost from one ACP usage update", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "sessionUpdate",
			update: { sessionUpdate: "usage_update", used: 50000, size: 200000, cost: { amount: 0.001035, currency: "USD" } },
		});
		await settle();

		expect(container.textContent).toContain("Context 25%");
		expect(container.textContent).toContain("USD 0.001035");
	});

	it("renders context occupancy without a missing cost", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({ kind: "sessionUpdate", update: { sessionUpdate: "usage_update", used: 100, size: 400 } });
		await settle();

		expect(container.textContent).toContain("Context 25%");
		expect(container.textContent).not.toContain("NaN");
		expect(container.textContent).not.toContain("USD");
	});

	it("does not carry a previous cost into a costless usage update", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "sessionUpdate",
			update: { sessionUpdate: "usage_update", used: 100, size: 400, cost: { amount: 2, currency: "USD" } },
		});
		feed({ kind: "sessionUpdate", update: { sessionUpdate: "usage_update", used: 200, size: 400 } });
		await settle();

		expect(container.textContent).toContain("Context 50%");
		expect(container.textContent).not.toContain("USD 2");
	});

	it("does not render a percentage for a zero context window", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({ kind: "sessionUpdate", update: { sessionUpdate: "usage_update", used: 100, size: 0 } });
		await settle();

		expect(container.textContent).not.toContain("Context");
		expect(container.textContent).not.toContain("Infinity");
	});

	it("does not replace a saved binding when config cannot be read", async () => {
		vi.mocked(invoke).mockRejectedValue(new Error("Config unavailable"));
		const { container } = await renderPanel();
		await settle();
		expect(client.newSession).not.toHaveBeenCalled();
		expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "save_config")).toBe(false);
		expect(container.textContent).toContain("Config unavailable");
	});

	it("opens a conversation when the agent does not advertise listing", async () => {
		client.connect.mockImplementation(async () => {
			const opened = snapshot({ capabilities: { ...snapshot().capabilities!, list: false } });
			acpStore.applySnapshot(opened);
			acpStore.markStreaming(CONNECTION);
			return opened;
		});
		client.newSession.mockImplementation(async () => {
			acpStore.applySnapshot(
				snapshot({
					capabilities: { ...snapshot().capabilities!, list: false },
					attachments: [attachment()],
				}),
			);
			return SESSION;
		});
		const { container } = await renderPanel();
		await settle();
		expect(client.newSession).toHaveBeenCalledWith(CONNECTION, CHAT_ROOT);
		expect(client.listSessions).not.toHaveBeenCalled();
		expect(container.querySelector("textarea")).not.toBeNull();
		const next = container.querySelector<HTMLButtonElement>(
			'button[aria-label="Start another conversation on this repository"]',
		);
		next?.click();
		await settle();
		expect(client.newSession).toHaveBeenCalledTimes(2);
		expect(client.listSessions).not.toHaveBeenCalled();
	});

	it("saves the selected session and restores it from the next document's config", async () => {
		const saved: Record<string, string> = {};
		vi.mocked(invoke).mockImplementation(async (command, args) => {
			if (command === "load_config") return { ai_chat_sessions: { ...saved } };
			if (command === "get_home_directory") return HOME;
			if (command === "save_config")
				Object.assign(
					saved,
					(args as { config: { ai_chat_sessions: Record<string, string> } }).config.ai_chat_sessions,
				);
			return undefined;
		});
		const first = await renderPanel();
		await settle();
		expect(saved[CHAT_ROOT]).toBe(SESSION);
		first.unmount();
		resetAcpChatBindings();
		vi.clearAllMocks();

		const second = renderIdlePanel();
		await settle();
		await typeAndSend(second.container, "go on");
		expect(client.loadSession).toHaveBeenCalledWith(CONNECTION, SESSION, CHAT_ROOT);
		expect(client.newSession).not.toHaveBeenCalled();
	});

	it("includes later ACP list pages before ordering the picker", async () => {
		client.listSessions.mockImplementation(async (_id, _root, cursor) =>
			cursor
				? {
						sessions: [{ sessionId: "newest", cwd: CHAT_ROOT, title: "New page", updatedAt: "2026-09-27T09:00:00Z" }],
						nextCursor: null,
					}
				: {
						sessions: [{ sessionId: SESSION, cwd: CHAT_ROOT, title: "First page", updatedAt: "2026-09-25T09:00:00Z" }],
						nextCursor: "page-2",
					},
		);
		const { container } = await renderPanel();
		await settle();
		const picker = container.querySelector('select[title="Conversation"]') as HTMLSelectElement;
		expect([...picker.options].map((option) => option.textContent)).toEqual(["New page", "First page"]);
	});

	it("keeps the current conversation visible when a picked load is refused", async () => {
		client.listSessions.mockResolvedValue({
			sessions: [
				{ sessionId: SESSION, cwd: CHAT_ROOT, title: "Current", updatedAt: "2026-09-26T09:00:00Z" },
				{ sessionId: "unavailable", cwd: CHAT_ROOT, title: "Unavailable", updatedAt: "2026-09-25T09:00:00Z" },
			],
			nextCursor: null,
		});
		client.loadSession.mockRejectedValue(new Error("Session unavailable"));
		const { container } = await renderPanel();
		await settle();
		const picker = container.querySelector('select[title="Conversation"]') as HTMLSelectElement;
		picker.value = "unavailable";
		picker.dispatchEvent(new Event("change", { bubbles: true }));
		await settle();
		expect(picker.value).toBe(SESSION);
		expect(container.textContent).toContain("Session unavailable");
	});

	it("loads the saved conversation after a fresh document opens", async () => {
		vi.mocked(invoke).mockImplementation(async (command) => {
			if (command === "load_config") return { ai_chat_sessions: { [CHAT_ROOT]: "prior-session" } };
			if (command === "get_home_directory") return HOME;
			return undefined;
		});
		client.listSessions.mockResolvedValue({
			sessions: [
				{ sessionId: "prior-session", cwd: CHAT_ROOT, title: "Design review", updatedAt: "2026-09-26T12:00:00Z" },
			],
			nextCursor: null,
		});
		client.loadSession.mockImplementation(async () => {
			acpStore.applySnapshot(snapshot({ attachments: [attachment({ sessionId: "prior-session" })] }));
			feed(
				{
					kind: "sessionUpdate",
					update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "Earlier answer" } },
				},
				"prior-session",
			);
		});

		const { container } = renderIdlePanel();
		await settle();
		expect(client.connect).not.toHaveBeenCalled();
		await typeAndSend(container, "go on");

		expect(client.newSession).not.toHaveBeenCalled();
		expect(client.loadSession).toHaveBeenCalledWith(CONNECTION, "prior-session", CHAT_ROOT);
		expect(container.textContent).toContain("Earlier answer");
	});

	it("lists durable conversation titles in latest activity order", async () => {
		client.listSessions.mockResolvedValue({
			sessions: [
				{ sessionId: "old", cwd: CHAT_ROOT, title: "Old topic", updatedAt: "2026-09-24T09:00:00Z" },
				{ sessionId: SESSION, cwd: CHAT_ROOT, title: "Current topic", updatedAt: "2026-09-25T09:00:00Z" },
				{ sessionId: "newest", cwd: CHAT_ROOT, title: "Latest topic", updatedAt: "2026-09-26T09:00:00Z" },
			],
			nextCursor: null,
		});
		const { container } = await renderPanel();
		await settle();

		const picker = container.querySelector('select[title="Conversation"]') as HTMLSelectElement;
		expect([...picker.options].map((option) => option.textContent)).toEqual([
			"Latest topic",
			"Current topic",
			"Old topic",
		]);
	});

	it("loads a picked conversation once and shows its replay without duplication", async () => {
		client.listSessions.mockResolvedValue({
			sessions: [
				{ sessionId: SESSION, cwd: CHAT_ROOT, title: "Current", updatedAt: "2026-09-25T09:00:00Z" },
				{ sessionId: "prior-session", cwd: CHAT_ROOT, title: "Earlier", updatedAt: "2026-09-24T09:00:00Z" },
			],
			nextCursor: null,
		});
		client.loadSession.mockImplementation(async () => {
			acpStore.applySnapshot(snapshot({ attachments: [attachment(), attachment({ sessionId: "prior-session" })] }));
			feed(
				{
					kind: "sessionUpdate",
					update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "Only once" } },
				},
				"prior-session",
			);
		});
		const { container } = await renderPanel();
		await settle();
		const picker = container.querySelector('select[title="Conversation"]') as HTMLSelectElement;
		picker.value = "prior-session";
		picker.dispatchEvent(new Event("change", { bubbles: true }));
		await settle();

		expect(client.loadSession).toHaveBeenCalledTimes(1);
		expect(client.loadSession).toHaveBeenCalledWith(CONNECTION, "prior-session", CHAT_ROOT);
		expect(container.textContent?.split("Only once")).toHaveLength(2);
	});
});

describe("AIChatPanel: tool activity", () => {
	function tool(id: number, status: "completed" | "failed" = "completed") {
		feed({
			kind: "sessionUpdate",
			update: {
				sessionUpdate: "tool_call",
				toolCallId: `call-${id}`,
				title: `Inspect file ${id}`,
				kind: "read",
				status,
				content: [{ type: "content", content: { type: "text", text: `output ${id}` } }],
			},
		});
	}

	it("keeps raw command text out of the collapsed row", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "sessionUpdate",
			update: {
				sessionUpdate: "tool_call",
				toolCallId: "bash",
				title: "bash -lc command -v tuic || true; ls tools/*",
				kind: "execute",
				status: "completed",
				content: [{ type: "content", content: { type: "text", text: "command output" } }],
			},
		});
		await settle();
		const group = container.querySelector("details[class*=toolActivity]") as HTMLDetailsElement;
		expect(group.querySelector(":scope > summary")?.textContent).toContain("1 tool call");
		expect(group.querySelector(":scope > summary")?.textContent).not.toContain("command -v");
		group.open = true;
		expect(group.textContent).toContain("command -v");
	});

	it("collapses seven calls in one turn into one activity line with count and salient titles", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "sessionUpdate",
			update: { sessionUpdate: "user_message_chunk", content: { type: "text", text: "Check files" } },
		});
		for (let id = 1; id <= 7; id += 1) tool(id);
		await settle();

		const summaries = [...container.querySelectorAll("summary")].filter((summary) =>
			summary.textContent?.includes("7 tool calls"),
		);
		expect(summaries).toHaveLength(1);
		expect(summaries[0].textContent).toContain("Inspect file 1");
		expect(summaries[0].textContent).toContain("Inspect file 2");
		expect(summaries[0].textContent).toMatch(/\d+(?:\.\d+)?s/);
		expect((summaries[0].parentElement as HTMLDetailsElement).open).toBe(false);
	});

	it("requires a second expansion before showing tool output", async () => {
		const { container } = await renderPanel();
		await settle();
		tool(1);
		await settle();

		const activity = [...container.querySelectorAll("details")].find((details) =>
			details.querySelector("summary")?.textContent?.includes("tool call"),
		);
		expect(activity).toBeDefined();
		expect(activity?.open).toBe(false);
		activity!.open = true;
		await settle();
		expect(activity?.textContent).toContain("Inspect file 1");
		expect(activity?.textContent).toContain("read");
		expect(activity?.textContent).toContain("Completed");
		const output = activity?.querySelector("details");
		expect(output?.open).toBe(false);
		expect(output?.textContent).toContain("output 1");
	});

	it("marks the activity line failed when any call fails", async () => {
		const { container } = await renderPanel();
		await settle();
		tool(1);
		tool(2, "failed");
		await settle();

		const summary = [...container.querySelectorAll("summary")].find((element) =>
			element.textContent?.includes("2 tool calls"),
		);
		expect(summary?.textContent).toContain("Failed");
	});

	it("keeps open permission and elicitation cards outside collapsed activity", async () => {
		const { container } = await renderPanel();
		await settle();
		tool(1);
		feed({
			kind: "permissionRequested",
			requestId: "req-activity",
			request: {
				sessionId: SESSION,
				toolCall: { title: "Write file" },
				options: [{ optionId: "reject", name: "Reject", kind: "reject_once" }],
			},
		});
		feed({
			kind: "elicitationRequested",
			requestId: "req-activity-form",
			request: {
				mode: "form",
				sessionId: SESSION,
				message: "Which branch?",
				requestedSchema: { type: "object", properties: { branch: { type: "string" } } },
			},
		});
		await settle();

		const activity = [...container.querySelectorAll("details")].find((details) =>
			details.querySelector("summary")?.textContent?.includes("tool call"),
		);
		expect(activity?.open).toBe(false);
		expect(container.textContent).toContain("Write file");
		expect(container.textContent).toContain("Which branch?");
		expect([...container.querySelectorAll("button")].find((button) => button.textContent === "Reject")).toBeDefined();
	});

	it("keeps calls together across agent text but separates the next user turn", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "sessionUpdate",
			update: { sessionUpdate: "user_message_chunk", content: { type: "text", text: "First task" } },
		});
		tool(1);
		feed({
			kind: "sessionUpdate",
			update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "Checking more." } },
		});
		tool(2);
		feed({
			kind: "sessionUpdate",
			update: { sessionUpdate: "user_message_chunk", content: { type: "text", text: "Second task" } },
		});
		tool(3);
		await settle();

		const summaries = [...container.querySelectorAll("summary")].filter((summary) =>
			summary.textContent?.includes("tool call"),
		);
		expect(summaries).toHaveLength(2);
		expect(summaries[0].textContent).toContain("2 tool calls");
		expect(summaries[1].textContent).toContain("1 tool call");
	});

	it("updates the collapsed line when an existing call later fails", async () => {
		const { container } = await renderPanel();
		await settle();
		tool(1);
		await settle();
		feed({
			kind: "sessionUpdate",
			update: { sessionUpdate: "tool_call_update", toolCallId: "call-1", status: "failed" },
		});
		await settle();

		const summary = [...container.querySelectorAll("summary")].find((element) =>
			element.textContent?.includes("1 tool call"),
		);
		expect(summary?.textContent).toContain("Failed");
	});

	it.each(["completed", "failed"] as const)(
		"stops pulsing both dots when a running call becomes %s",
		async (status) => {
			const { container } = await renderPanel();
			await settle();
			feed({
				kind: "sessionUpdate",
				update: { sessionUpdate: "tool_call", toolCallId: "transition", title: "Inspect file", status: "pending" },
			});
			await settle();

			const activity = container.querySelector("details[class*=toolActivity]") as HTMLDetailsElement;
			activity.open = true;
			const dots = () => [...activity.querySelectorAll("summary > span:first-child")];
			expect(dots()).toHaveLength(2);
			for (const dot of dots()) expect(dot.className).toContain("toolCallPending");

			feed({
				kind: "sessionUpdate",
				update: { sessionUpdate: "tool_call_update", toolCallId: "transition", status },
			});
			await settle();

			for (const dot of dots()) {
				expect(dot.className).not.toContain("toolCallPending");
				expect(dot.className).toContain(status === "failed" ? "toolCallFailure" : "toolCallSuccess");
			}
		},
	);

	it.each([
		{ event: { kind: "turnSettled", stopReason: "end_turn", usage: null } as const, status: "completed" },
		{ event: { kind: "turnFailed", message: "tool process exited", state: "idle" } as const, status: "failed" },
	])("stops pulsing an unfinished tool call when the turn ends as $status", async ({ event, status }) => {
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "sessionUpdate",
			update: { sessionUpdate: "tool_call", toolCallId: "unfinished", title: "Inspect file", status: "in_progress" },
		});
		await settle();
		const activity = container.querySelector("details[class*=toolActivity]") as HTMLDetailsElement;
		activity.open = true;
		const dots = () => [...activity.querySelectorAll("summary > span:first-child")];
		expect(dots()).toHaveLength(2);
		for (const dot of dots()) expect(dot.className).toContain("toolCallPending");

		feed(event);
		await settle();
		for (const dot of dots()) {
			expect(dot.className).not.toContain("toolCallPending");
			expect(dot.className).toContain(status === "failed" ? "toolCallFailure" : "toolCallSuccess");
		}
	});

	it("extends observed duration when a later call joins an already completed activity", async () => {
		const realNow = performance.now.bind(performance);
		const now = vi.spyOn(performance, "now");
		let elapsed = 0;
		now.mockImplementation(() => realNow() + elapsed);
		try {
			const { container } = await renderPanel();
			await settle();
			tool(1);
			await settle();
			elapsed = 2500;
			tool(2);
			await settle();

			const summary = [...container.querySelectorAll("summary")].find((element) =>
				element.textContent?.includes("2 tool calls"),
			);
			expect(summary?.textContent).toMatch(/2\.5s observed|2\.6s observed/);
		} finally {
			now.mockRestore();
		}
	});
});

describe("AIChatPanel: permission", () => {
	it("keeps Allow always visibly enabled and answers with its published id", async () => {
		const { container } = await renderPanel();
		feed({
			kind: "permissionRequested",
			requestId: "req-persistent",
			request: {
				sessionId: SESSION,
				toolCall: { title: "Edit src/main.rs" },
				options: [
					{ optionId: "approve-this-run", name: "Allow once", kind: "allow_once" },
					{ optionId: "persist-rule", name: "Allow always", kind: "allow_always" },
				],
			},
		});
		await settle();

		const always = [...container.querySelectorAll("button")].find((button) => button.textContent === "Allow always");
		expect(always).toBeDefined();
		expect(always?.disabled).toBe(false);
		always?.click();
		await settle();
		expect(client.answerPermission).toHaveBeenCalledWith(CONNECTION, "req-persistent", "persist-rule");

		const stylesheet = readFileSync(resolve(process.cwd(), "src/components/AIChatPanel/AIChatPanel.module.css"), "utf8");
		const alwaysStyle = /\.alwaysAllowBtn\s*\{([^}]*)\}/.exec(stylesheet)?.[1];
		expect(alwaysStyle, "persistent approval must use the enabled success color").toContain("var(--success)");
		expect(alwaysStyle).not.toContain("var(--fg-muted)");
	});

	// The option list is the agent's. Answering with anything but one of its own
	// option ids answers a question nobody asked.
	it("renders the options ego published and answers with one of their ids", async () => {
		const { container } = await renderPanel();
		await settle();

		feed({
			kind: "permissionRequested",
			requestId: "req-1",
			request: {
				sessionId: SESSION,
				toolCall: { title: "Write src/main.rs" },
				options: [
					{ optionId: "allow-once", name: "Allow once", kind: "allow_once" },
					{ optionId: "reject-once", name: "Reject", kind: "reject_once" },
				],
			},
		});
		await settle();

		expect(container.textContent).toContain("Write src/main.rs");
		const allow = [...container.querySelectorAll("button")].find((button) => button.textContent === "Allow once");
		expect(allow).toBeDefined();
		allow?.click();
		await settle();

		expect(client.answerPermission).toHaveBeenCalledWith(CONNECTION, "req-1", "allow-once");
	});
});

describe("AIChatPanel: elicitation", () => {
	it("keeps an elicitation with an additional unsupported field in the form", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "elicitationRequested",
			requestId: "mixed-form",
			request: {
				mode: "form",
				sessionId: SESSION,
				message: "Choose and describe",
				requestedSchema: {
					type: "object",
					properties: { answer: { type: "string", enum: ["yes", "no"] }, context: { type: "array" } },
				},
			},
		});
		await settle();
		expect(container.querySelector("select.formInput")).not.toBeNull();
		expect(container.textContent).toContain("Submit");
	});

	it("keeps a four-choice elicitation in the form", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "elicitationRequested",
			requestId: "choice-4",
			request: {
				mode: "form",
				sessionId: SESSION,
				message: "Choose one",
				requestedSchema: {
					type: "object",
					properties: { answer: { type: "string", enum: ["one", "two", "three", "four"] } },
					required: ["answer"],
				},
			},
		});
		await settle();
		expect(container.querySelector("select.formInput")).not.toBeNull();
		expect(container.textContent).toContain("Submit");
	});

	it("offers small single-select trust choices as direct buttons plus Cancel", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({ kind: "turnStarted" }, SESSION, "turn-1");
		feed({
			kind: "elicitationRequested",
			requestId: "trust-1",
			request: {
				mode: "form",
				sessionId: SESSION,
				message: "Trust this workspace?",
				requestedSchema: {
					type: "object",
					properties: { answer: { type: "string", enum: ["trusted", "untrusted"] } },
					required: ["answer"],
				},
			},
		});
		await settle();
		expect(container.querySelector("select.formInput")).toBeNull();
		expect(container.textContent).not.toContain("Submit");
		const trust = [...container.querySelectorAll("button")].find((button) => button.textContent === "Trust");
		expect(trust).toBeDefined();
		expect([...container.querySelectorAll("button")].some((button) => button.textContent === "Don't trust")).toBe(true);
		expect([...container.querySelectorAll("button")].some((button) => button.textContent === "Cancel")).toBe(true);
		trust?.click();
		await settle();
		expect(client.answerElicitation).toHaveBeenCalledWith(CONNECTION, "trust-1", {
			action: "accept",
			content: { answer: "trusted" },
		});
		feed({
			kind: "elicitationSettled",
			requestId: "trust-1",
			action: { action: "accept", content: { answer: "trusted" } },
		});
		feed({ kind: "turnFailed", message: "model unavailable", state: "idle" }, SESSION, "turn-1");
		await settle();
		expect(container.textContent).toContain("model unavailable");
		expect([...container.querySelectorAll("button")].some((button) => button.textContent === "Send")).toBe(true);
	});

	it("renders a form and submits the values that were filled in", async () => {
		const { container } = await renderPanel();
		await settle();

		feed({
			kind: "elicitationRequested",
			requestId: "req-2",
			request: {
				mode: "form",
				sessionId: SESSION,
				message: "Which branch should I use?",
				requestedSchema: {
					type: "object",
					properties: { branch: { type: "string", title: "Branch" } },
					required: ["branch"],
				},
			},
		});
		await settle();

		expect(container.textContent).toContain("Which branch should I use?");
		const input = container.querySelector('input[type="text"]') as HTMLInputElement;
		input.value = "main";
		input.dispatchEvent(new Event("input", { bubbles: true }));
		const submit = [...container.querySelectorAll("button")].find((button) => button.textContent === "Submit");
		submit?.click();
		await settle();

		expect(client.answerElicitation).toHaveBeenCalledWith(CONNECTION, "req-2", {
			action: "accept",
			content: { branch: "main" },
		});
	});

	// The Rust client answers `cancel` to every mode but `form` before it reaches
	// a host, so a mode this client never advertised must never be drawn — a form
	// for it would collect values the agent cannot read back.
	it("draws nothing for a mode this client did not advertise", async () => {
		const { container } = await renderPanel();
		await settle();

		feed({
			kind: "elicitationRequested",
			requestId: "req-3",
			request: { mode: "confirm", sessionId: SESSION, message: "Proceed?", requestedSchema: {} },
		} as unknown as AcpClientEvent);
		await settle();

		expect(container.textContent).not.toContain("Proceed?");
	});
});

describe("elicitationFields", () => {
	it("reads a field per property, with its title and whether it is required", () => {
		expect(
			elicitationFields({
				type: "object",
				properties: {
					branch: { type: "string", title: "Branch" },
					depth: { type: "integer" },
					force: { type: "boolean" },
					mode: { type: "string", enum: ["fast", "safe"] },
				},
				required: ["branch"],
			}),
		).toEqual([
			{ name: "branch", label: "Branch", type: "string", choices: [], required: true },
			{ name: "depth", label: "depth", type: "number", choices: [], required: false },
			{ name: "force", label: "force", type: "boolean", choices: [], required: false },
			{ name: "mode", label: "mode", type: "enum", choices: ["fast", "safe"], required: false },
		]);
	});

	// A nested object has no single control to draw, and guessing one would
	// collect a value the agent cannot read back.
	it("skips a property it cannot draw one control for", () => {
		expect(
			elicitationFields({ type: "object", properties: { nested: { type: "object" }, list: { type: "array" } } }),
		).toEqual([]);
	});

	it("reads no field out of a schema it cannot understand", () => {
		expect(elicitationFields(null)).toEqual([]);
		expect(elicitationFields({ type: "string" })).toEqual([]);
	});
});

describe("AIChatPanel: a gap", () => {
	it("replays every open tab after the ACP connection is replaced", async () => {
		client.newSession.mockResolvedValueOnce(SESSION).mockResolvedValueOnce(SECOND_SESSION);
		const { container } = await renderPanel();
		await settle();
		(container.querySelector('button[aria-label="New chat tab"]') as HTMLButtonElement).click();
		await settle();
		acpStore.applyFrame(CONNECTION, {
			kind: "gap",
			code: "stream_gap",
			message: "journal expired",
			connectionId: CONNECTION,
			sessionId: null,
			operation: null,
			retryable: false,
		});
		await settle();
		(
			[...container.querySelectorAll("button")].find((button) => button.textContent === "Recover") as HTMLButtonElement
		).click();
		await settle();
		expect(client.loadSession).toHaveBeenCalledWith(CONNECTION, SECOND_SESSION, CHAT_ROOT);
		expect(client.loadSession).toHaveBeenCalledWith(CONNECTION, SESSION, CHAT_ROOT);
	});
	// A gap says the journal no longer holds what the cursor asks for. Skipping
	// ahead would leave a hole in the conversation that nothing on screen admits
	// to; the recovery on record is a fresh process replaying the history.
	it("surfaces the gap and offers the recovery", async () => {
		const { container } = await renderPanel();
		await settle();

		acpStore.applyFrame(CONNECTION, {
			kind: "gap",
			code: "stream_gap",
			message: "sequence 3 is no longer held",
			connectionId: CONNECTION,
			sessionId: null,
			operation: null,
			retryable: false,
		});
		await settle();

		expect(container.textContent).toContain("Missed part of this conversation");
		const recover = [...container.querySelectorAll("button")].find((button) => button.textContent === "Recover");
		recover?.click();
		await settle();

		expect(client.reconnect).toHaveBeenCalledWith(CONNECTION, CHAT_ROOT);
		expect(client.loadSession).toHaveBeenCalledWith(CONNECTION, SESSION, CHAT_ROOT);
	});
});

describe("AIChatPanel: the session's own knobs", () => {
	const mode: AcpSessionConfigOption = {
		id: "mode",
		name: "Mode",
		description: "How ego handles tools",
		type: "select",
		currentValue: "ask",
		options: [
			{ value: "ask", name: "Ask" },
			{ value: "auto", name: "Automatic" },
		],
	};

	// Catches: an ACP option is hidden or shown without its published name and choice.
	it("shows every published select with a label, description, and current choice in a dialog", async () => {
		const { container } = await renderPanel();
		await settle();
		acpStore.applySnapshot(snapshot({ attachments: [attachment({ configOptions: [MODEL_OPTION, mode] })] }));
		await settle();

		expect(container.textContent).toContain("Opus");
		expect(container.textContent).toContain("Ask");
		expect(container.querySelectorAll(".controlBar select")).toHaveLength(0);
		(container.querySelector('button[aria-label="Session settings"]') as HTMLButtonElement).click();
		const dialog = container.querySelector('[role="dialog"]') as HTMLElement;
		expect(dialog).not.toBeNull();
		expect(dialog.textContent).toContain("How ego handles tools");
		expect(
			(dialog.querySelector('select[aria-label="Model"]') as HTMLSelectElement).selectedOptions[0].textContent,
		).toBe("Opus");
		expect(
			(dialog.querySelector('select[aria-label="Mode"]') as HTMLSelectElement).selectedOptions[0].textContent,
		).toBe("Ask");
	});

	it("renders ACP grouped choices with their group label and selected value", async () => {
		const grouped: AcpSessionConfigOption = {
			id: "mode",
			name: "Mode",
			type: "select",
			currentValue: "auto",
			options: [
				{
					group: "behavior",
					name: "Behavior",
					options: [
						{ value: "ask", name: "Ask" },
						{ value: "auto", name: "Automatic" },
					],
				},
			],
		};
		const { container } = await renderPanel();
		await settle();
		acpStore.applySnapshot(snapshot({ attachments: [attachment({ configOptions: [grouped] })] }));
		await settle();
		expect(container.querySelector(".controlBar")?.textContent).toContain("Mode: Automatic");
		(container.querySelector('button[aria-label="Session settings"]') as HTMLButtonElement).click();
		const picker = container.querySelector('select[aria-label="Mode"]') as HTMLSelectElement;
		expect(picker.querySelector("optgroup")?.label).toBe("Behavior");
		expect(picker.selectedOptions[0].value).toBe("auto");
		expect(picker.selectedOptions[0].textContent).toBe("Automatic");
	});

	// Catches: a new tab keeps a blank select after its options arrive.
	it("shows the current choice when a second tab receives its options after opening", async () => {
		client.newSession.mockResolvedValueOnce(SESSION).mockResolvedValueOnce(SECOND_SESSION);
		const { container } = await renderPanel();
		await settle();
		(container.querySelector('button[aria-label="New chat tab"]') as HTMLButtonElement).click();
		await settle();
		acpStore.applySnapshot(
			snapshot({
				attachments: [
					attachment(),
					attachment({ sessionId: SECOND_SESSION, configOptions: [{ ...MODEL_OPTION, currentValue: "sonnet" }] }),
				],
			}),
		);
		await settle();
		(container.querySelector('button[aria-label="Session settings"]') as HTMLButtonElement).click();
		const picker = container.querySelector('select[aria-label="Model"]') as HTMLSelectElement;
		expect(picker.selectedOptions[0].textContent).toBe("Sonnet");
	});

	// Catches: the summary optimistically reports a choice ego has not accepted.
	it("sends a changed choice and updates the summary only from the agent's reply", async () => {
		const { container } = await renderPanel();
		await settle();
		(container.querySelector('button[aria-label="Session settings"]') as HTMLButtonElement).click();
		const picker = container.querySelector('select[aria-label="Model"]') as HTMLSelectElement;

		picker.value = "sonnet";
		picker.dispatchEvent(new Event("change", { bubbles: true }));
		await settle();
		expect(client.setConfigOption).toHaveBeenCalledWith(CONNECTION, SESSION, "model", { value: "sonnet" });
		expect(container.querySelector(".controlBar")?.textContent).toContain("Opus");
		acpStore.applySnapshot(
			snapshot({ attachments: [attachment({ configOptions: [{ ...MODEL_OPTION, currentValue: "sonnet" }] })] }),
		);
		await settle();
		expect(container.querySelector(".controlBar")?.textContent).toContain("Sonnet");
	});

	// Catches: a rejected choice appears accepted and its error is lost.
	it("shows a rejected setting change inside the dialog", async () => {
		client.setConfigOption.mockRejectedValueOnce(new Error("Model unavailable"));
		const { container } = await renderPanel();
		await settle();
		(container.querySelector('button[aria-label="Session settings"]') as HTMLButtonElement).click();
		const picker = container.querySelector('select[aria-label="Model"]') as HTMLSelectElement;
		picker.value = "sonnet";
		picker.dispatchEvent(new Event("change", { bubbles: true }));
		await settle();
		expect(container.querySelector('[role="dialog"]')?.textContent).toContain("Model unavailable");
		expect(picker.value).toBe("opus");
	});
});

describe("AIChatPanel: pause, resume and compact", () => {
	// Catches: text controls wrapping below a long model summary or icons losing accessible names.
	it("keeps named icon controls on one row beside a short model summary", async () => {
		const style = document.createElement("style");
		style.textContent = readFileSync(
			resolve(process.cwd(), "src/components/AIChatPanel/AIChatPanel.module.css"),
			"utf8",
		);
		document.head.append(style);
		try {
			const { container } = await renderPanel();
			await settle();
			acpStore.applySnapshot(
				snapshot({
					attachments: [
						attachment({
							state: "prompting",
							configOptions: [
								{
									...MODEL_OPTION,
									currentValue: "openai-codex/gpt-6-sol",
									options: [{ value: "openai-codex/gpt-6-sol", name: "openai-codex/gpt-6-sol" }],
								},
							],
						}),
					],
				}),
			);
			await settle();
			const bar = container.querySelector<HTMLElement>(".controlBar")!;
			expect(bar.querySelector(".sessionSettingsSummary")?.textContent).toBe("Model: gpt-6-sol");
			expect(getComputedStyle(bar).flexWrap).toBe("nowrap");
			for (const label of [
				"Pause the turn",
				"Compact the conversation",
				"Start another conversation on this repository",
			]) {
				const button = bar.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)!;
				expect(button.title).toBe(label);
				expect(button.querySelector("svg")).not.toBeNull();
				expect(button.textContent?.trim()).toBe("");
			}
			acpStore.applySnapshot(snapshot({ attachments: [attachment({ state: "paused" })] }));
			await settle();
			const resume = bar.querySelector<HTMLButtonElement>('button[aria-label="Resume the turn"]')!;
			expect(resume.title).toBe("Resume the turn");
			expect(resume.querySelector("svg")).not.toBeNull();
		} finally {
			style.remove();
		}
	});

	it("pauses a running turn and resumes a held one", async () => {
		const { container } = await renderPanel();
		await settle();

		acpStore.applySnapshot(snapshot({ attachments: [attachment({ state: "prompting" })] }));
		await settle();

		const pause = container.querySelector<HTMLButtonElement>('button[aria-label="Pause the turn"]');
		pause?.click();
		await settle();
		expect(client.pause).toHaveBeenCalledWith(CONNECTION, SESSION);

		acpStore.applySnapshot(snapshot({ attachments: [attachment({ state: "paused" })] }));
		await settle();

		const resume = container.querySelector<HTMLButtonElement>('button[aria-label="Resume the turn"]');
		resume?.click();
		await settle();
		expect(client.resumeTurn).toHaveBeenCalledWith(CONNECTION, SESSION);
	});

	it("compacts the conversation", async () => {
		const { container } = await renderPanel();
		await settle();

		const compact = container.querySelector<HTMLButtonElement>('button[aria-label="Compact the conversation"]');
		compact?.click();
		await settle();

		expect(client.compact).toHaveBeenCalledWith(CONNECTION, SESSION);
	});

	// An ego that did not advertise the extension gets no button, rather than a
	// button that fails when it is pressed.
	it("offers neither when the agent did not advertise them", async () => {
		client.connect.mockImplementation(async () => {
			const opened = snapshot({
				capabilities: { ...snapshot().capabilities!, egoHoldVersion: null, egoCompactVersion: null },
			});
			acpStore.applySnapshot(opened);
			acpStore.markStreaming(CONNECTION);
			return opened;
		});
		client.newSession.mockImplementation(async () => {
			acpStore.applySnapshot(
				snapshot({
					attachments: [attachment()],
					capabilities: { ...snapshot().capabilities!, egoHoldVersion: null, egoCompactVersion: null },
				}),
			);
			return SESSION;
		});

		const { container } = await renderPanel();
		await settle();

		expect(container.querySelector('button[aria-label="Pause the turn"]')).toBeNull();
		expect(container.querySelector('button[aria-label="Compact the conversation"]')).toBeNull();
	});
});
