import { render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const smartPromptsMocks = vi.hoisted(() => ({
	executeSmartPrompt: vi.fn().mockResolvedValue({ ok: true }),
	canExecute: vi.fn(() => ({ ok: true })),
}));

// Replaces the real useSmartPrompts hook — executing a real headless prompt
// needs a much larger dependency graph (agentConfigsStore, providerRegistryStore,
// IPC commands) out of scope here; that mechanism is covered by
// useSmartPrompts.test.ts. This test is only about what ChangesTab's own
// "Generate commit message" button passes INTO executeSmartPrompt.
vi.mock("../../hooks/useSmartPrompts", async (importOriginal) => {
	const actual = await importOriginal<typeof import("../../hooks/useSmartPrompts")>();
	return {
		...actual,
		useSmartPrompts: () => ({
			executeSmartPrompt: smartPromptsMocks.executeSmartPrompt,
			canExecute: smartPromptsMocks.canExecute,
			resolveAllVariables: vi.fn().mockResolvedValue({}),
		}),
	};
});

import { ChangesTab } from "../../components/GitPanel/ChangesTab";
import { promptLibraryStore, type SavedPrompt } from "../../stores/promptLibrary";
import { repositoriesStore } from "../../stores/repositories";
import { mockInvoke } from "../mocks/tauri";

function smartCommitMsgPrompt(): SavedPrompt {
	return {
		id: "smart-commit-msg",
		name: "Generate Commit Message",
		content: "Generate a commit message",
		category: "custom",
		isFavorite: false,
		createdAt: 1,
		updatedAt: 1,
		executionMode: "headless",
	};
}

describe("ChangesTab — 'Generate commit message' targets its own repo", () => {
	// Regression coverage for the cross-repo bug fixed alongside SmartButtonStrip's:
	// this button calls executeSmartPrompt directly (not through SmartButtonStrip),
	// so it needed its own fix and needs its own coverage that the fix reached it.
	const REPO_PATH = "/repo/for/changes-tab-smart-prompt";

	beforeEach(() => {
		// Fake timers, matching ChangesTabConflicts.test.tsx: the commit
		// textarea's ref schedules a real `requestAnimationFrame` (autoResize)
		// that otherwise outlives the test as an async-leak false positive —
		// `vi.runOnlyPendingTimers()` in afterEach flushes it deterministically.
		vi.useFakeTimers();
		mockInvoke.mockReset().mockResolvedValue({
			staged: [{ path: "a.txt", status: "M", additions: 1, deletions: 0 }],
			unstaged: [],
			untracked: [],
			conflicted: [],
		});
		repositoriesStore.add({ path: REPO_PATH, displayName: "Repo" });
		vi.spyOn(promptLibraryStore, "getSmartByPlacement").mockReturnValue([smartCommitMsgPrompt()]);
	});

	afterEach(() => {
		vi.runOnlyPendingTimers();
		vi.useRealTimers();
		vi.restoreAllMocks();
		repositoriesStore.remove(REPO_PATH);
		repositoriesStore._testCancelPendingSave();
	});

	it("passes its own repoPath as executeSmartPrompt's targetPath when clicked", async () => {
		const { getByTitle } = render(() => <ChangesTab repoPath={REPO_PATH} onOpenDiff={vi.fn()} />);

		const button = await waitFor(() => {
			const btn = getByTitle("Generate commit message") as HTMLButtonElement;
			expect(btn.disabled).toBe(false);
			return btn;
		});
		button.click();

		await waitFor(() => {
			expect(smartPromptsMocks.executeSmartPrompt).toHaveBeenCalledWith(
				expect.objectContaining({ id: "smart-commit-msg" }),
				undefined,
				REPO_PATH,
			);
		});
	});
});
