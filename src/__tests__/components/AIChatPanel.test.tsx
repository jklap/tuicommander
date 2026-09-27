// @vitest-environment jsdom
//
// The panel renders an agent's answer through ContentRenderer, whose DOMPurify
// pass needs a complete NodeIterator; happy-dom's is not.

import { cleanup, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { mockDetachPanel, mockReattachPanel, mockClosePanel } = vi.hoisted(() => ({
	mockDetachPanel: vi.fn().mockResolvedValue(undefined),
	mockReattachPanel: vi.fn().mockResolvedValue(undefined),
	mockClosePanel: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/api/core", () => ({
	invoke: vi.fn().mockResolvedValue(undefined),
	Channel: vi.fn(),
	convertFileSrc: (path: string) => path,
}));

vi.mock("@tauri-apps/api/event", () => ({
	listen: vi.fn().mockResolvedValue(vi.fn()),
	emit: vi.fn().mockResolvedValue(undefined),
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
import { AIChatPanel } from "../../components/AIChatPanel/AIChatPanel";
import { aiChatDraft } from "../../components/AIChatPanel/draft";
import { elicitationFields } from "../../components/AIChatPanel/Interactions";
import { resetAcpChatBindings } from "../../components/AIChatPanel/useAcpChat";
import { acpStore } from "../../stores/acp";
import { acpTranscript } from "../../stores/acpTranscript";
import type {
	AcpAttachmentSnapshot,
	AcpClientEvent,
	AcpConnectionSnapshot,
	AcpSessionConfigOption,
} from "../../types/acp";

const ROOT = "/repo/tuicommander";
const CONNECTION = "01932d5e-0000-7000-8000-0000000000c1";
const SESSION = "01932d5e-0000-7000-8000-0000000000aa";
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
		{ id: "opus", name: "Opus" },
		{ id: "sonnet", name: "Sonnet" },
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
function feed(event: AcpClientEvent, sessionId: string | null = SESSION): void {
	sequence += 1;
	const frame = {
		kind: "event" as const,
		connectionId: CONNECTION,
		generation: 1,
		sequence,
		sessionId,
		turnId: null,
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

function renderPanel() {
	return render(() => <AIChatPanel visible={true} repoPath={ROOT} onClose={() => {}} />);
}

beforeEach(() => {
	vi.clearAllMocks();
	vi.mocked(invoke).mockImplementation(async (command) => {
		if (command === "load_config") return { ai_chat_sessions: {} };
		return undefined;
	});
	sequence = 0;
	supportsImages = false;
	settings.egoExecutable = "/usr/local/bin/ego";
	acpStore.reset();
	acpTranscript.reset();
	resetAcpChatBindings();
	aiChatDraft.clear();

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
	client.reconnect.mockImplementation(async () => {
		const opened = snapshot({ attachments: [attachment()] });
		acpStore.applySnapshot(opened);
		acpStore.markStreaming(CONNECTION);
		return opened;
	});
	for (const method of [
		"disconnect",
		"loadSession",
		"cancel",
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
		const { container } = renderPanel();
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
		const { container } = renderPanel();
		await settle();

		expect(container.textContent).toContain("tuicommander");
	});
});

describe("AIChatPanel: without a configured binary", () => {
	// An empty `ego_executable` is refused in Rust at connect. Saying so is the
	// difference between a panel that explains itself and one that silently
	// never starts.
	it("explains that ACP is not configured and launches nothing", async () => {
		settings.egoExecutable = "";
		const { container } = renderPanel();
		await settle();

		expect(container.textContent).toContain("ACP is not configured");
		expect(client.connect).not.toHaveBeenCalled();
		expect(container.querySelector("textarea")).toBeNull();
	});
});

describe("AIChatPanel: a turn", () => {
	// Catches: an image paste is ignored even though ego advertises image prompts.
	it("stages a pasted PNG and sends it with the next turn", async () => {
		supportsImages = true;
		const { container } = renderPanel();
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
		expect(client.prompt).toHaveBeenCalledWith(CONNECTION, SESSION, "", [
			{ type: "image", mimeType: "image/png", data: PNG_1X1 },
		]);
	});

	// Catches: a paste is accepted for an agent that cannot read image blocks.
	it("refuses an image when the connection did not advertise image prompts", async () => {
		const { container } = renderPanel();
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
		const { container } = renderPanel();
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
		const { container } = renderPanel();
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
		const { container } = renderPanel();
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
		expect(client.prompt).toHaveBeenCalledWith(CONNECTION, SESSION, "text only");
	});

	// Catches: intercepting all paste events breaks the browser's text insertion.
	it("leaves plain text paste to the textarea", async () => {
		const { container } = renderPanel();
		await settle();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		const event = new Event("paste", { bubbles: true, cancelable: true });
		Object.defineProperty(event, "clipboardData", { value: { items: [], getData: () => "ordinary text" } });
		textarea.dispatchEvent(event);
		expect(event.defaultPrevented).toBe(false);
	});
	it("opens a connection on the repo root and a session on the same root", async () => {
		renderPanel();
		await settle();

		expect(client.connect).toHaveBeenCalledWith(ROOT);
		expect(client.newSession).toHaveBeenCalledWith(CONNECTION, ROOT);
	});

	it("sends what was typed and streams the answer back", async () => {
		const { container } = renderPanel();
		await settle();

		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		textarea.value = "what does acp/mod.rs do?";
		textarea.dispatchEvent(new Event("input", { bubbles: true }));
		await settle();

		const send = [...container.querySelectorAll("button")].find((button) => button.textContent === "Send");
		send?.click();
		await settle();

		expect(client.prompt).toHaveBeenCalledWith(CONNECTION, SESSION, "what does acp/mod.rs do?");

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
		const first = renderPanel();
		await settle();
		first.unmount();

		renderPanel();
		await settle();

		expect(client.connect).toHaveBeenCalledTimes(1);
		expect(client.newSession).toHaveBeenCalledTimes(1);
	});
});

describe("AIChatPanel: durable conversations", () => {
	it("does not replace a saved binding when config cannot be read", async () => {
		vi.mocked(invoke).mockRejectedValue(new Error("Config unavailable"));
		const { container } = renderPanel();
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
		const { container } = renderPanel();
		await settle();
		expect(client.newSession).toHaveBeenCalledWith(CONNECTION, ROOT);
		expect(client.listSessions).not.toHaveBeenCalled();
		expect(container.querySelector("textarea")).not.toBeNull();
		const next = [...container.querySelectorAll("button")].find((button) => button.textContent === "New");
		next?.click();
		await settle();
		expect(client.newSession).toHaveBeenCalledTimes(2);
		expect(client.listSessions).not.toHaveBeenCalled();
	});

	it("saves the selected session and restores it from the next document's config", async () => {
		const saved: Record<string, string> = {};
		vi.mocked(invoke).mockImplementation(async (command, args) => {
			if (command === "load_config") return { ai_chat_sessions: { ...saved } };
			if (command === "save_config")
				Object.assign(
					saved,
					(args as { config: { ai_chat_sessions: Record<string, string> } }).config.ai_chat_sessions,
				);
			return undefined;
		});
		const first = renderPanel();
		await settle();
		expect(saved[ROOT]).toBe(SESSION);
		first.unmount();
		resetAcpChatBindings();
		vi.clearAllMocks();

		renderPanel();
		await settle();
		expect(client.loadSession).toHaveBeenCalledWith(CONNECTION, SESSION, ROOT);
		expect(client.newSession).not.toHaveBeenCalled();
	});

	it("includes later ACP list pages before ordering the picker", async () => {
		client.listSessions.mockImplementation(async (_id, _root, cursor) =>
			cursor
				? {
						sessions: [{ sessionId: "newest", cwd: ROOT, title: "New page", updatedAt: "2026-09-27T09:00:00Z" }],
						nextCursor: null,
					}
				: {
						sessions: [{ sessionId: SESSION, cwd: ROOT, title: "First page", updatedAt: "2026-09-25T09:00:00Z" }],
						nextCursor: "page-2",
					},
		);
		const { container } = renderPanel();
		await settle();
		const picker = container.querySelector('select[title="Conversation"]') as HTMLSelectElement;
		expect([...picker.options].map((option) => option.textContent)).toEqual(["New page", "First page"]);
	});

	it("keeps the current conversation visible when a picked load is refused", async () => {
		client.listSessions.mockResolvedValue({
			sessions: [
				{ sessionId: SESSION, cwd: ROOT, title: "Current", updatedAt: "2026-09-26T09:00:00Z" },
				{ sessionId: "unavailable", cwd: ROOT, title: "Unavailable", updatedAt: "2026-09-25T09:00:00Z" },
			],
			nextCursor: null,
		});
		client.loadSession.mockRejectedValue(new Error("Session unavailable"));
		const { container } = renderPanel();
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
			if (command === "load_config") return { ai_chat_sessions: { [ROOT]: "prior-session" } };
			return undefined;
		});
		client.listSessions.mockResolvedValue({
			sessions: [{ sessionId: "prior-session", cwd: ROOT, title: "Design review", updatedAt: "2026-09-26T12:00:00Z" }],
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

		const { container } = renderPanel();
		await settle();

		expect(client.newSession).not.toHaveBeenCalled();
		expect(client.loadSession).toHaveBeenCalledWith(CONNECTION, "prior-session", ROOT);
		expect(container.textContent).toContain("Earlier answer");
	});

	it("lists durable conversation titles in latest activity order", async () => {
		client.listSessions.mockResolvedValue({
			sessions: [
				{ sessionId: "old", cwd: ROOT, title: "Old topic", updatedAt: "2026-09-24T09:00:00Z" },
				{ sessionId: SESSION, cwd: ROOT, title: "Current topic", updatedAt: "2026-09-25T09:00:00Z" },
				{ sessionId: "newest", cwd: ROOT, title: "Latest topic", updatedAt: "2026-09-26T09:00:00Z" },
			],
			nextCursor: null,
		});
		const { container } = renderPanel();
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
				{ sessionId: SESSION, cwd: ROOT, title: "Current", updatedAt: "2026-09-25T09:00:00Z" },
				{ sessionId: "prior-session", cwd: ROOT, title: "Earlier", updatedAt: "2026-09-24T09:00:00Z" },
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
		const { container } = renderPanel();
		await settle();
		const picker = container.querySelector('select[title="Conversation"]') as HTMLSelectElement;
		picker.value = "prior-session";
		picker.dispatchEvent(new Event("change", { bubbles: true }));
		await settle();

		expect(client.loadSession).toHaveBeenCalledTimes(1);
		expect(client.loadSession).toHaveBeenCalledWith(CONNECTION, "prior-session", ROOT);
		expect(container.textContent?.split("Only once")).toHaveLength(2);
	});
});

describe("AIChatPanel: permission", () => {
	// The option list is the agent's. Answering with anything but one of its own
	// option ids answers a question nobody asked.
	it("renders the options ego published and answers with one of their ids", async () => {
		const { container } = renderPanel();
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
	it("renders a form and submits the values that were filled in", async () => {
		const { container } = renderPanel();
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
		const { container } = renderPanel();
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
	// A gap says the journal no longer holds what the cursor asks for. Skipping
	// ahead would leave a hole in the conversation that nothing on screen admits
	// to; the recovery on record is a fresh process replaying the history.
	it("surfaces the gap and offers the recovery", async () => {
		const { container } = renderPanel();
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

		expect(client.reconnect).toHaveBeenCalledWith(CONNECTION, ROOT);
		expect(client.loadSession).toHaveBeenCalledWith(CONNECTION, SESSION, ROOT);
	});
});

describe("AIChatPanel: the session's own knobs", () => {
	// Model, effort and mode are the session's vocabulary. A list of models in
	// the panel would be a second, wrong answer to a question ego already
	// answers, and it would go stale the first time ego learned a new one.
	it("renders the published options and sets one through set_config_option", async () => {
		const { container } = renderPanel();
		await settle();

		const picker = [...container.querySelectorAll("select")].find((select) =>
			[...select.options].some((option) => option.textContent === "Sonnet"),
		) as HTMLSelectElement;
		expect(picker.value).toBe("opus");

		picker.value = "sonnet";
		picker.dispatchEvent(new Event("change", { bubbles: true }));
		await settle();

		expect(client.setConfigOption).toHaveBeenCalledWith(CONNECTION, SESSION, "model", { value: "sonnet" });
	});
});

describe("AIChatPanel: pause, resume and compact", () => {
	it("pauses a running turn and resumes a held one", async () => {
		const { container } = renderPanel();
		await settle();

		acpStore.applySnapshot(snapshot({ attachments: [attachment({ state: "prompting" })] }));
		await settle();

		const pause = [...container.querySelectorAll("button")].find((button) => button.textContent === "Pause");
		pause?.click();
		await settle();
		expect(client.pause).toHaveBeenCalledWith(CONNECTION, SESSION);

		acpStore.applySnapshot(snapshot({ attachments: [attachment({ state: "paused" })] }));
		await settle();

		const resume = [...container.querySelectorAll("button")].find((button) => button.textContent === "Resume");
		resume?.click();
		await settle();
		expect(client.resumeTurn).toHaveBeenCalledWith(CONNECTION, SESSION);
	});

	it("compacts the conversation", async () => {
		const { container } = renderPanel();
		await settle();

		const compact = [...container.querySelectorAll("button")].find((button) => button.textContent === "Compact");
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

		const { container } = renderPanel();
		await settle();

		const labels = [...container.querySelectorAll("button")].map((button) => button.textContent);
		expect(labels).not.toContain("Pause");
		expect(labels).not.toContain("Compact");
	});
});
