import { EditorView } from "@codemirror/view";
import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const pending = vi.hoisted(() => ({ send: vi.fn() }));

vi.mock("../../../hooks/usePty", () => ({
	usePty: () => ({
		createSession: vi.fn().mockResolvedValue("sess-toggle"),
		sendCommand: pending.send,
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
	invoke: vi.fn(async (cmd: string) =>
		cmd === "get_session_foreground_process"
			? "claude"
			: cmd === "get_shell_state"
				? "busy"
				: cmd === "chat_view_snapshot"
					? { epoch: 0, nextSeq: 0, reset: true, updates: [], unknownRows: 0, malformedRows: 0 }
					: undefined,
	),
	listen: vi.fn(async () => () => {}),
}));

import { Terminal } from "../../../components/Terminal/Terminal";
import { appLogger } from "../../../stores/appLogger";
import { terminalsStore } from "../../../stores/terminals";

beforeEach(() => {
	vi.spyOn(appLogger, "debug").mockImplementation(() => {});
	for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
	vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
	vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(600);
});

afterEach(async () => {
	cleanup();
	// Real CodeMirror leaves a measurement frame and a 10ms blur timer on destroy.
	await new Promise<void>((resolve) => setImmediate(resolve));
	await new Promise<void>((resolve) => setTimeout(resolve, 20));
});

describe("Chat draft during delayed send", () => {
	it("keeps_the_next_unsent_message_when_leaving_chat_before_send_settles", async () => {
		let finish!: () => void;
		pending.send.mockImplementation(
			() =>
				new Promise<void>((resolve) => {
					finish = resolve;
				}),
		);
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
		terminalsStore.setViewMode(id, "chat");
		await waitFor(() => expect(view.container.querySelector(".cm-editor")).not.toBeNull());
		await new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
		const editor = EditorView.findFromDOM(view.container.querySelector<HTMLElement>(".cm-editor")!);
		if (!editor) throw new Error("No Compose editor");
		editor.dispatch({ changes: { from: 0, insert: "Sent message" } });
		fireEvent.click(view.getByTitle("Send (Ctrl+Enter)"));
		editor.dispatch({ changes: { from: editor.state.doc.length, insert: "\nNext unsent message" } });
		terminalsStore.setViewMode(id, "cli");
		finish();
		await new Promise<void>((resolve) => setImmediate(resolve));
		terminalsStore.setViewMode(id, "chat");
		await waitFor(() => {
			const restored = view.container.querySelector<HTMLElement>(".cm-editor");
			expect(restored && EditorView.findFromDOM(restored)?.state.doc.toString()).toContain("Next unsent message");
		});
	});
});
