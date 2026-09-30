import { beforeEach, describe, expect, it, vi } from "vitest";

/**
 * Without this adapter the AI Chat panel has no registry entry, and every way
 * of opening it is a silent no-op: `togglePanel("ai-chat")` returns at
 * `if (!adapter?.toggle) return false`, which is what `Cmd+Alt+A`, the
 * status-bar button, the command-palette entry and the detach control all go
 * through. That failure is invisible — no error, no log, nothing on screen —
 * so it is worth a test of its own.
 */

const ui = vi.hoisted(() => ({
	toggleAiChatPanel: vi.fn(),
	setAiChatPanelVisible: vi.fn(),
}));

const repositories = vi.hoisted(() => ({
	state: { activeRepoPath: null as string | null },
	getActive: vi.fn(() => null as null | Record<string, unknown>),
}));

vi.mock("../../stores/ui", () => ({ uiStore: ui }));
vi.mock("../../stores/repositories", () => ({ repositoriesStore: repositories }));
vi.mock("../../components/AIChatPanel/AIChatPanel", () => ({ AIChatPanel: () => null }));
vi.mock("../../hooks/initPanelWindow", () => ({ initPanelWindow: vi.fn() }));

import { aiChatPanelAdapter } from "../../panelAdapters/aiChat";

beforeEach(() => {
	vi.clearAllMocks();
	repositories.state.activeRepoPath = null;
	repositories.getActive.mockReturnValue(null);
});

describe("the AI Chat panel adapter", () => {
	it("registers under the id every opener uses", () => {
		expect(aiChatPanelAdapter.id).toBe("ai-chat");
		expect(aiChatPanelAdapter.toggle).toBeDefined();
	});

	it("toggles the panel", () => {
		aiChatPanelAdapter.toggle?.();
		expect(ui.toggleAiChatPanel).toHaveBeenCalled();
	});

	it("hides the inline panel when it moves into its own window", () => {
		aiChatPanelAdapter.onDetach?.();
		expect(ui.setAiChatPanelVisible).toHaveBeenCalledWith(false);
	});

	// A detached panel is a separate WebView where `repositoriesStore` is never
	// hydrated. It has to be handed the repository, or it can never open a
	// connection at all.
	it("hands the detached window the repository it must talk about", () => {
		repositories.state.activeRepoPath = "/repo/tuicommander";
		repositories.getActive.mockReturnValue({
			path: "/repo/tuicommander",
			activeWorkspaceId: "w1",
			workspaces: { w1: { worktreePath: "/repo/tuicommander-wt" } },
		});

		expect(aiChatPanelAdapter.detachParams?.()).toEqual({
			repoPath: "/repo/tuicommander",
			fsRoot: "/repo/tuicommander-wt",
		});
	});

	it("passes no repository when there is none", () => {
		expect(aiChatPanelAdapter.detachParams?.()).toEqual({});
	});
});
