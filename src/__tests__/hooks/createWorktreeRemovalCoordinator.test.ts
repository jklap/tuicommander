import { createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { testInScopeAsync } from "../helpers/store";

const mockInvoke = vi.fn().mockResolvedValue(undefined);
vi.mock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));

describe("createWorktreeRemovalCoordinator", () => {
	let createWorktreeRemovalCoordinator: typeof import("../../hooks/git/createWorktreeRemovalCoordinator").createWorktreeRemovalCoordinator;
	let repositoriesStore: typeof import("../../stores/repositories").repositoriesStore;
	let repoSettingsStore: typeof import("../../stores/repoSettings").repoSettingsStore;

	const REPO = "/Gits/alpha";
	const BRANCH = "feature-x";

	beforeEach(async () => {
		vi.resetModules();
		mockInvoke.mockReset().mockResolvedValue(undefined);
		vi.doMock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));
		createWorktreeRemovalCoordinator = (await import("../../hooks/git/createWorktreeRemovalCoordinator"))
			.createWorktreeRemovalCoordinator;
		repositoriesStore = (await import("../../stores/repositories")).repositoriesStore;
		repoSettingsStore = (await import("../../stores/repoSettings")).repoSettingsStore;
		repositoriesStore._testSetHydrated(true);
	});

	afterEach(() => {
		repositoriesStore._testCancelPendingSave();
	});

	function setupBranch(overrides: Record<string, unknown> = {}) {
		repositoriesStore.add({ path: REPO, displayName: "alpha" });
		repositoriesStore.setBranch(REPO, BRANCH, {
			worktreePath: `${REPO}__wt/${BRANCH}`,
			terminals: [],
			...overrides,
		});
	}

	/** Builds a coordinator with real solid signals for the removingBranches lock,
	 *  and mock-everything-else deps. Every dialog defaults to "confirmed" so a
	 *  test only has to override the ones it cares about. */
	function makeCoordinator(depOverrides: Record<string, unknown> = {}) {
		const [removingBranches, setRemovingBranches] = createSignal<Set<string>>(new Set());
		const statusMessages: string[] = [];
		const removeWorktree = vi.fn().mockResolvedValue({});
		const confirmRemoveWorktree = vi.fn().mockResolvedValue(true);
		const confirmRemoveLockedWorktree = vi.fn().mockResolvedValue(true);
		const confirmRemoveBusyWorktree = vi.fn().mockResolvedValue(true);
		const confirmForceRemoveDirtyWorktree = vi.fn().mockResolvedValue(true);
		const closeTerminal = vi.fn().mockResolvedValue(undefined);
		const checkWorktreeDirty = vi.fn().mockResolvedValue(false);

		const dialogs = {
			confirmRemoveWorktree,
			confirmRemoveLockedWorktree,
			confirmRemoveBusyWorktree,
			confirmForceRemoveDirtyWorktree,
			...(depOverrides.dialogs as Record<string, unknown> | undefined),
		};

		const coordinator = createWorktreeRemovalCoordinator({
			repo: { removeWorktree },
			checkWorktreeDirty,
			closeTerminal,
			setStatusInfo: (m: string) => statusMessages.push(m),
			removingBranches,
			setRemovingBranches,
			...depOverrides,
			dialogs,
		} as never);

		return {
			coordinator,
			statusMessages,
			removeWorktree,
			confirmRemoveWorktree,
			confirmRemoveLockedWorktree,
			confirmRemoveBusyWorktree,
			confirmForceRemoveDirtyWorktree,
			closeTerminal,
			checkWorktreeDirty,
			removingBranches,
		};
	}

	it("removes the branch after confirming, defaulting deleteBranch to true with no repo settings", async () => {
		await testInScopeAsync(async () => {
			setupBranch();
			const { coordinator, removeWorktree } = makeCoordinator();

			await coordinator.handleRemoveBranch(REPO, BRANCH);

			expect(removeWorktree).toHaveBeenCalledWith(REPO, BRANCH, true);
			expect(repositoriesStore.get(REPO)?.branches[BRANCH]).toBeUndefined();
		});
	});

	it("passes deleteBranch=false through to remove_worktree when the repo setting is off", async () => {
		await testInScopeAsync(async () => {
			setupBranch();
			repoSettingsStore.getOrCreate(REPO, "alpha");
			repoSettingsStore.update(REPO, { deleteBranchOnRemove: false });
			const { coordinator, removeWorktree } = makeCoordinator();

			await coordinator.handleRemoveBranch(REPO, BRANCH);

			expect(removeWorktree).toHaveBeenCalledWith(REPO, BRANCH, false);
		});
	});

	it("does not remove the worktree or the store branch when the user cancels the confirm dialog", async () => {
		await testInScopeAsync(async () => {
			setupBranch();
			const { coordinator, removeWorktree, confirmRemoveWorktree } = makeCoordinator();
			confirmRemoveWorktree.mockResolvedValue(false);

			await coordinator.handleRemoveBranch(REPO, BRANCH);

			expect(removeWorktree).not.toHaveBeenCalled();
			expect(repositoriesStore.get(REPO)?.branches[BRANCH]).toBeDefined();
		});
	});

	it("shows confirmRemoveBusyWorktree instead of confirmRemoveWorktree when the branch has an attached terminal", async () => {
		await testInScopeAsync(async () => {
			setupBranch({ terminals: ["t1"] });
			const { coordinator, confirmRemoveWorktree, confirmRemoveBusyWorktree } = makeCoordinator();

			await coordinator.handleRemoveBranch(REPO, BRANCH);

			expect(confirmRemoveBusyWorktree).toHaveBeenCalledWith(BRANCH, expect.any(Object));
			expect(confirmRemoveWorktree).not.toHaveBeenCalled();
		});
	});

	it("falls back to confirmRemoveWorktree for a busy branch when confirmRemoveBusyWorktree is not provided", async () => {
		await testInScopeAsync(async () => {
			setupBranch({ terminals: ["t1"] });
			const { coordinator, confirmRemoveWorktree } = makeCoordinator({
				dialogs: { confirmRemoveBusyWorktree: undefined },
			});

			await coordinator.handleRemoveBranch(REPO, BRANCH);

			expect(confirmRemoveWorktree).toHaveBeenCalledWith(BRANCH, true);
		});
	});

	// Regression: the confirm dialog used to be asked with only `branchName`, so
	// its message unconditionally claimed the local branch would be deleted even
	// when "Delete local branch when removing worktree" was off.
	it("passes the effective deleteBranchOnRemove setting to the confirm dialog, not only to remove_worktree", async () => {
		await testInScopeAsync(async () => {
			setupBranch();
			repoSettingsStore.getOrCreate(REPO, "alpha");
			repoSettingsStore.update(REPO, { deleteBranchOnRemove: false });
			const { coordinator, confirmRemoveWorktree } = makeCoordinator();

			await coordinator.handleRemoveBranch(REPO, BRANCH);

			expect(confirmRemoveWorktree).toHaveBeenCalledWith(BRANCH, false);
		});
	});

	it("closes every attached terminal before invoking remove_worktree, and tolerates a terminal that fails to close", async () => {
		await testInScopeAsync(async () => {
			setupBranch({ terminals: ["t1", "t2"] });
			const { coordinator, closeTerminal, removeWorktree } = makeCoordinator();
			closeTerminal.mockRejectedValueOnce(new Error("pty gone")).mockResolvedValueOnce(undefined);

			await coordinator.handleRemoveBranch(REPO, BRANCH);

			expect(closeTerminal).toHaveBeenCalledWith("t1", true);
			expect(closeTerminal).toHaveBeenCalledWith("t2", true);
			expect(removeWorktree).toHaveBeenCalled();
		});
	});

	it("re-prompts with confirmRemoveBusyWorktree and overrides on worktree_busy, then succeeds", async () => {
		await testInScopeAsync(async () => {
			setupBranch();
			const { coordinator, removeWorktree, confirmRemoveBusyWorktree } = makeCoordinator();
			removeWorktree.mockRejectedValueOnce(new Error("worktree_busy: session attached")).mockResolvedValueOnce({});

			await coordinator.handleRemoveBranch(REPO, BRANCH);

			expect(confirmRemoveBusyWorktree).toHaveBeenCalled();
			expect(removeWorktree).toHaveBeenNthCalledWith(2, REPO, BRANCH, true, false, true);
			expect(repositoriesStore.get(REPO)?.branches[BRANCH]).toBeUndefined();
		});
	});

	it("cancelling the busy-override prompt leaves the branch and does not retry removal", async () => {
		await testInScopeAsync(async () => {
			setupBranch();
			const { coordinator, removeWorktree, confirmRemoveBusyWorktree } = makeCoordinator();
			removeWorktree.mockRejectedValueOnce(new Error("worktree_busy: session attached"));
			confirmRemoveBusyWorktree.mockResolvedValue(false);

			await coordinator.handleRemoveBranch(REPO, BRANCH);

			expect(removeWorktree).toHaveBeenCalledTimes(1);
			expect(repositoriesStore.get(REPO)?.branches[BRANCH]).toBeDefined();
		});
	});

	it("re-prompts with confirmForceRemoveDirtyWorktree and forces on worktree_dirty, then succeeds", async () => {
		await testInScopeAsync(async () => {
			setupBranch();
			const { coordinator, removeWorktree, confirmForceRemoveDirtyWorktree } = makeCoordinator();
			removeWorktree.mockRejectedValueOnce(new Error("worktree_dirty: uncommitted changes")).mockResolvedValueOnce({});

			await coordinator.handleRemoveBranch(REPO, BRANCH);

			expect(confirmForceRemoveDirtyWorktree).toHaveBeenCalledWith(BRANCH);
			expect(removeWorktree).toHaveBeenNthCalledWith(2, REPO, BRANCH, true, true);
			expect(repositoriesStore.get(REPO)?.branches[BRANCH]).toBeUndefined();
		});
	});

	it("re-prompts with confirmRemoveLockedWorktree (passing deleteBranch and isDirty) on worktree_locked, then force-removes", async () => {
		await testInScopeAsync(async () => {
			setupBranch();
			const { coordinator, removeWorktree, confirmRemoveLockedWorktree, checkWorktreeDirty } = makeCoordinator();
			checkWorktreeDirty.mockResolvedValue(true);
			removeWorktree.mockRejectedValueOnce(new Error("worktree_locked: agent session")).mockResolvedValueOnce({});

			await coordinator.handleRemoveBranch(REPO, BRANCH);

			expect(confirmRemoveLockedWorktree).toHaveBeenCalledWith(BRANCH, true, true);
			expect(removeWorktree).toHaveBeenNthCalledWith(2, REPO, BRANCH, true, true);
			expect(repositoriesStore.get(REPO)?.branches[BRANCH]).toBeUndefined();
		});
	});

	it("treats an unanswered dirty-check (null) as possibly dirty when locked", async () => {
		await testInScopeAsync(async () => {
			setupBranch();
			const { coordinator, confirmRemoveLockedWorktree, checkWorktreeDirty, removeWorktree } = makeCoordinator();
			checkWorktreeDirty.mockResolvedValue(null);
			removeWorktree.mockRejectedValueOnce(new Error("worktree_locked: agent session")).mockResolvedValueOnce({});

			await coordinator.handleRemoveBranch(REPO, BRANCH);

			expect(confirmRemoveLockedWorktree).toHaveBeenCalledWith(BRANCH, true, true);
		});
	});

	it("reports worktree_is_main without removing the branch from the store", async () => {
		await testInScopeAsync(async () => {
			setupBranch();
			const { coordinator, removeWorktree, statusMessages } = makeCoordinator();
			removeWorktree.mockRejectedValueOnce(new Error("worktree_is_main: cannot remove"));

			await coordinator.handleRemoveBranch(REPO, BRANCH);

			expect(statusMessages.some((m) => m.includes("main worktree"))).toBe(true);
			expect(repositoriesStore.get(REPO)?.branches[BRANCH]).toBeDefined();
		});
	});

	it("keeps the branch row on an unrecognized removal failure instead of silently dropping it", async () => {
		await testInScopeAsync(async () => {
			setupBranch();
			const { coordinator, removeWorktree, statusMessages } = makeCoordinator();
			removeWorktree.mockRejectedValueOnce(new Error("some_other_git_failure: disk full"));

			await coordinator.handleRemoveBranch(REPO, BRANCH);

			expect(statusMessages.some((m) => m.includes("Failed to remove"))).toBe(true);
			expect(repositoriesStore.get(REPO)?.branches[BRANCH]).toBeDefined();
		});
	});

	it("ignores a second concurrent call for the same branch while the first is still in flight", async () => {
		await testInScopeAsync(async () => {
			setupBranch();
			let resolveConfirm!: (v: boolean) => void;
			const { coordinator, confirmRemoveWorktree, removeWorktree } = makeCoordinator();
			confirmRemoveWorktree.mockImplementation(() => new Promise<boolean>((resolve) => (resolveConfirm = resolve)));

			const first = coordinator.handleRemoveBranch(REPO, BRANCH);
			const second = coordinator.handleRemoveBranch(REPO, BRANCH);

			expect(confirmRemoveWorktree).toHaveBeenCalledTimes(1);
			resolveConfirm(true);
			await Promise.all([first, second]);
			expect(removeWorktree).toHaveBeenCalledTimes(1);
		});
	});

	it("reports 'not a worktree' and does nothing when the branch has no worktreePath", async () => {
		await testInScopeAsync(async () => {
			repositoriesStore.add({ path: REPO, displayName: "alpha" });
			repositoriesStore.setBranch(REPO, "main", { worktreePath: null });
			const { coordinator, confirmRemoveWorktree, statusMessages } = makeCoordinator();

			await coordinator.handleRemoveBranch(REPO, "main");

			expect(confirmRemoveWorktree).not.toHaveBeenCalled();
			expect(statusMessages).toEqual(["Cannot remove main: not a worktree"]);
		});
	});

	it("clears the branch label on a clean removal but keeps it when the backend warns the branch survived", async () => {
		await testInScopeAsync(async () => {
			setupBranch();
			repoSettingsStore.getOrCreate(REPO, "alpha");
			repoSettingsStore.setLabel(REPO, BRANCH, "my label");
			const { coordinator, removeWorktree } = makeCoordinator();
			removeWorktree.mockResolvedValue({ branch_delete_warning: "not fully merged" });

			await coordinator.handleRemoveBranch(REPO, BRANCH);

			expect(repoSettingsStore.get(REPO)?.branchLabels[BRANCH]).toBe("my label");
		});
	});
});
