import { beforeEach, describe, expect, it, vi } from "vitest";

/**
 * The terminal's AI entries, which the panel's rebuild had to bring back.
 *
 * They write a question into the composer instead of sending it: ego is
 * launched lazily by the panel, so sending here would race a connection that is
 * still coming up. And they do not bind the conversation to the terminal that
 * was right-clicked — the panel is bound to a repository and a session.
 */

const terminals = vi.hoisted(() => ({
	state: { terminals: {} as Record<string, unknown> },
}));

const ui = vi.hoisted(() => ({ setAiChatPanelVisible: vi.fn() }));

vi.mock("../../stores/terminals", () => ({ terminalsStore: terminals }));
vi.mock("../../stores/ui", () => ({ uiStore: ui }));
vi.mock("../../stores/appLogger", () => ({
	appLogger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

import { registerAiChatContextActions, truncateText } from "../../components/AIChatPanel/contextMenuActions";
import { aiChatDraft } from "../../components/AIChatPanel/draft";
import { contextMenuActionsStore } from "../../stores/contextMenuActionsStore";

/** One terminal holding a selection and a buffer, as the store keeps it. */
function terminal(selection: string, lines: string[] = []) {
	return {
		sessionId: "sess-1",
		ref: {
			getSelection: () => selection,
			getBufferLines: async () => lines,
		},
	};
}

function action(id: string) {
	return contextMenuActionsStore.getContextActions("terminal").find((entry) => entry.id === id);
}

let registered: Array<{ dispose(): void }> = [];

beforeEach(() => {
	vi.clearAllMocks();
	registered.forEach((disposable) => disposable.dispose());
	aiChatDraft.clear();
	terminals.state.terminals = { t1: terminal("") };
	registered = registerAiChatContextActions();
});

describe("truncateText", () => {
	it("leaves text that fits alone", () => {
		expect(truncateText("short", 10)).toBe("short");
	});

	it("says where it cut", () => {
		expect(truncateText("abcdef", 3)).toBe("abc\n[... truncated]");
	});
});

describe("the AI entries on a terminal", () => {
	it("registers both, under the ids they always had", () => {
		expect(contextMenuActionsStore.getContextActions("terminal").map((entry) => entry.id)).toEqual(
			expect.arrayContaining(["ai-chat:explain", "ai-chat:fix-error"]),
		);
	});

	it("asks about the selection and opens the panel", async () => {
		terminals.state.terminals = { t1: terminal("error: no such file") };

		action("ai-chat:explain")?.action({ target: "terminal", sessionId: "sess-1" });
		await new Promise((resolve) => setTimeout(resolve, 0));

		expect(aiChatDraft.text()).toContain("error: no such file");
		expect(aiChatDraft.text()).toContain("Explain this terminal output");
		expect(ui.setAiChatPanelVisible).toHaveBeenCalledWith(true);
	});

	// Nothing selected is the common case: a person right-clicks after watching
	// a command fail, and what they mean is what is on the screen.
	it("falls back to the tail of the screen when nothing is selected", async () => {
		terminals.state.terminals = { t1: terminal("", ["cargo build", "error[E0433]: failed to resolve"]) };

		action("ai-chat:fix-error")?.action({ target: "terminal", sessionId: "sess-1" });
		await new Promise((resolve) => setTimeout(resolve, 0));

		expect(aiChatDraft.text()).toContain("error[E0433]: failed to resolve");
		expect(aiChatDraft.text()).toContain("suggest a fix");
	});

	// A terminal with nothing in it has nothing to ask about, and opening the
	// panel on an empty question would look like the entry misfired.
	it("writes nothing and opens nothing when there is no output", async () => {
		terminals.state.terminals = { t1: terminal("", []) };

		action("ai-chat:explain")?.action({ target: "terminal", sessionId: "sess-1" });
		await new Promise((resolve) => setTimeout(resolve, 0));

		expect(aiChatDraft.text()).toBe("");
		expect(ui.setAiChatPanelVisible).not.toHaveBeenCalled();
	});
});
