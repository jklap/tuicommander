import { createRoot } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { mockAppLogger, mockPromptLibraryStore } = vi.hoisted(() => ({
	mockAppLogger: { error: vi.fn() },
	mockPromptLibraryStore: { getSmartByPlacement: vi.fn() },
}));

vi.mock("../../stores/appLogger", () => ({ appLogger: mockAppLogger }));
vi.mock("../../stores/promptLibrary", () => ({ promptLibraryStore: mockPromptLibraryStore }));

import { usePluginContextActions } from "../../hooks/usePluginContextActions";
import { contextMenuActionsStore } from "../../stores/contextMenuActionsStore";
import type { SavedPrompt } from "../../stores/promptLibrary";

const branchPrompt = {
	id: "branch-prompt",
	name: "Review branch",
} as SavedPrompt;
const terminalPrompt = {
	id: "terminal-prompt",
	name: "Explain terminal",
} as SavedPrompt;

describe("usePluginContextActions", () => {
	let dispose: (() => void) | undefined;
	const executeSmartPrompt = vi.fn().mockResolvedValue({ ok: true });
	const canExecute = vi.fn(() => ({ ok: false, reason: "busy" }));

	beforeEach(() => {
		contextMenuActionsStore.clear();
		executeSmartPrompt.mockClear();
		canExecute.mockClear();
		mockAppLogger.error.mockClear();
		mockPromptLibraryStore.getSmartByPlacement.mockImplementation((placement: string) => {
			if (placement === "git-branches") return [branchPrompt];
			if (placement === "terminal-context") return [terminalPrompt];
			return [];
		});
	});

	afterEach(() => {
		dispose?.();
		dispose = undefined;
		contextMenuActionsStore.clear();
	});

	it("registers smart prompts with their target-specific behavior", async () => {
		createRoot((rootDispose) => {
			dispose = rootDispose;
			usePluginContextActions({ executeSmartPrompt, canExecute });
		});

		const branchAction = contextMenuActionsStore.getContextActions("branch")[0];
		const terminalAction = contextMenuActionsStore.getContextActions("terminal")[0];
		branchAction.action({ target: "branch", branchName: "feature/test" });
		terminalAction.action({ target: "terminal" });
		await Promise.resolve();

		expect(branchAction).toMatchObject({ id: "smart:branch-prompt", label: "Review branch" });
		expect(terminalAction).toMatchObject({ id: "smart:terminal-prompt", label: "Explain terminal" });
		expect(executeSmartPrompt).toHaveBeenNthCalledWith(1, branchPrompt, { branch_name: "feature/test" });
		expect(executeSmartPrompt).toHaveBeenNthCalledWith(2, terminalPrompt);
		expect(terminalAction.disabled?.({ target: "terminal" })).toBe(true);
		expect(canExecute).toHaveBeenCalledWith(terminalPrompt);
	});

	it("disposes every registration when the owning root is disposed", () => {
		createRoot((rootDispose) => {
			dispose = rootDispose;
			usePluginContextActions({ executeSmartPrompt, canExecute });
		});
		expect(contextMenuActionsStore.getContextActions("branch")).toHaveLength(1);

		dispose?.();
		dispose = undefined;
		expect(contextMenuActionsStore.getContextActions("branch")).toEqual([]);
		expect(contextMenuActionsStore.getContextActions("terminal")).toEqual([]);
	});
});
