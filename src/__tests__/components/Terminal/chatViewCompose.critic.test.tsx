import { EditorView } from "@codemirror/view";
import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../../hooks/usePty", () => ({
	usePty: () => ({
		createSession: vi.fn().mockResolvedValue("sess-toggle"),
		resize: vi.fn(),
		close: vi.fn(),
		getKittyFlags: vi.fn().mockResolvedValue(0),
	}),
}));
vi.mock("../../../transport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../../transport")>()),
	rpc: vi.fn().mockResolvedValue(undefined),
	subscribePty: () => Promise.resolve(() => {}),
}));
vi.mock("@tauri-apps/api/core", () => ({
	invoke: vi.fn(async (cmd: string) => (cmd === "get_session_foreground_process" ? "claude" : undefined)),
}));
vi.mock("../../../stores/agentConfigs", () => ({
	ensureAgentConfigsForRepo: vi.fn().mockResolvedValue({ getEnvFlags: () => ({}) }),
	agentConfigsForRepo: () => ({ getEnvFlags: () => ({}) }),
}));
vi.mock("../../../components/Terminal/glyphCache", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../../components/Terminal/glyphCache")>()),
	getSharedMetrics: () => ({ cellWidth: 8, cellHeight: 16 }),
}));
vi.mock("../../../components/Terminal/CanvasTerminal", () => ({
	default: () => <div data-testid="canvas" />,
}));
vi.mock("../../../invoke", () => ({
	invoke: vi.fn(async () => ({
		epoch: 0,
		nextSeq: 0,
		reset: true,
		updates: [],
		unknownRows: 0,
		malformedRows: 0,
	})),
	listen: vi.fn(async () => () => {}),
}));

import { Terminal } from "../../../components/Terminal/Terminal";
import { terminalsStore } from "../../../stores/terminals";

beforeEach(() => {
	vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
	vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(600);
});

afterEach(async () => {
	cleanup();
	// Real CodeMirror leaves a measurement frame and a 10ms blur timer on destroy.
	await new Promise<void>((resolve) => setImmediate(resolve));
	await new Promise<void>((resolve) => setTimeout(resolve, 20));
});

describe("Chat with an existing Compose editor", () => {
	// Catches: an open editor remaining interactive in Chat, or its draft being lost on return to CLI.
	it("hides_an_already_open_compose_editor_in_chat_and_restores_its_draft_in_cli", async () => {
		const id = terminalsStore.add({
			sessionId: "live-session",
			cwd: "/repo",
			repoPath: "/repo",
			name: "claude",
			fontSize: 13,
			awaitingInput: null,
		});
		terminalsStore.update(id, { agentType: "claude", agentSessionId: "uuid" });
		const view = render(() => <Terminal id={id} cwd="/repo" alwaysVisible />);
		fireEvent.click(view.getByText(/^Compose /));
		await view.findByLabelText("Close compose panel");
		await waitFor(() => expect(view.container.querySelector(".cm-editor")).not.toBeNull());
		// Opening initializes CodeMirror over two frames; type after that setup has finished.
		await new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
		const editor = view.container.querySelector<HTMLElement>(".cm-editor");
		if (!editor) throw new Error("Compose editor did not mount");
		const codeMirror = EditorView.findFromDOM(editor);
		if (!codeMirror) throw new Error("Compose CodeMirror view is unavailable");
		const draft = "Keep this unsent draft\nincluding its second line.";
		codeMirror.dispatch({ changes: { from: 0, to: codeMirror.state.doc.length, insert: draft } });
		expect(codeMirror.state.doc.toString()).toBe(draft);
		terminalsStore.setViewMode(id, "chat");
		await Promise.resolve();
		expect(view.queryByLabelText("Close compose panel")).toBeNull();
		expect(view.container.querySelector(".cm-editor")).toBeNull();
		terminalsStore.setViewMode(id, "cli");
		await view.findByLabelText("Close compose panel");
		await waitFor(() => {
			const restored = view.container.querySelector<HTMLElement>(".cm-editor");
			expect(restored && EditorView.findFromDOM(restored)?.state.doc.toString()).toBe(draft);
		});
	});
});
