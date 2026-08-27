import { createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { testInScopeAsync } from "../helpers/store";

const mockInvoke = vi.fn().mockResolvedValue(undefined);
vi.mock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));

/**
 * Direct characterization of `createWorktreeRemovalCoordinator` (it was only
 * reached through `useGitOperations` before). Written against main's removal
 * model: a backend lifecycle preflight, one confirmation that knows the
 * requested branch action, a separate Cancel-by-default "in use" question for
 * a workspace with attached terminals, and a separate lock override.
 */
describe("createWorktreeRemovalCoordinator", () => {
	let createWorktreeRemovalCoordinator: typeof import("../../hooks/git/createWorktreeRemovalCoordinator").createWorktreeRemovalCoordinator;
	let repositoriesStore: typeof import("../../stores/repositories").repositoriesStore;
	let repoSettingsStore: typeof import("../../stores/repoSettings").repoSettingsStore;

	const REPO = "/Gits/alpha";
	const WORKSPACE = "feature-x";

	const SAFE_LIFECYCLE = { dirtyFiles: 0, commitStatus: "in_sync", removalSafety: "safe" };

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

	function setupWorkspace(overrides: Record<string, unknown> = {}) {
		repositoriesStore.add({ path: REPO, displayName: "alpha" });
		repositoriesStore.setWorkspace(REPO, WORKSPACE, {
			worktreePath: `${REPO}__wt/${WORKSPACE}`,
			terminals: [],
			...overrides,
		});
	}

	/** A coordinator with real solid signals for the removingBranches lock and
	 *  mock-everything-else deps. Every dialog defaults to "confirmed". */
	function makeCoordinator(depOverrides: { dialogs?: Record<string, unknown>; lifecycle?: unknown } = {}) {
		const [removingBranches, setRemovingBranches] = createSignal<Set<string>>(new Set());
		const statusMessages: string[] = [];
		const removeWorktree = vi.fn().mockResolvedValue({});
		const getWorkspaceLifecycle = vi.fn().mockResolvedValue(depOverrides.lifecycle ?? SAFE_LIFECYCLE);
		const confirmRemoveWorktree = vi.fn().mockResolvedValue(true);
		const confirmRemoveLockedWorktree = vi.fn().mockResolvedValue(true);
		const confirmRemoveBusyWorktree = vi.fn().mockResolvedValue(true);
		const closeTerminal = vi.fn().mockResolvedValue(undefined);

		const coordinator = createWorktreeRemovalCoordinator({
			repo: { removeWorktree, getWorkspaceLifecycle },
			dialogs: {
				confirmRemoveWorktree,
				confirmRemoveLockedWorktree,
				confirmRemoveBusyWorktree,
				...depOverrides.dialogs,
			},
			closeTerminal,
			setStatusInfo: (m: string) => statusMessages.push(m),
			removingBranches,
			setRemovingBranches,
		} as never);

		return {
			coordinator,
			statusMessages,
			removeWorktree,
			getWorkspaceLifecycle,
			confirmRemoveWorktree,
			confirmRemoveLockedWorktree,
			confirmRemoveBusyWorktree,
			closeTerminal,
		};
	}

	it("removes the workspace after confirming, defaulting deleteBranch to true with no repo settings", async () => {
		await testInScopeAsync(async () => {
			setupWorkspace();
			const { coordinator, removeWorktree, confirmRemoveWorktree } = makeCoordinator();

			await coordinator.handleRemoveWorkspace(REPO, WORKSPACE);

			expect(confirmRemoveWorktree).toHaveBeenCalledWith(WORKSPACE, SAFE_LIFECYCLE, true);
			expect(removeWorktree).toHaveBeenCalledWith(REPO, WORKSPACE, true, false);
			expect(repositoriesStore.get(REPO)?.workspaces[WORKSPACE]).toBeUndefined();
		});
	});

	it("passes the effective deleteBranchOnRemove=false to the confirm dialog AND to remove_worktree", async () => {
		await testInScopeAsync(async () => {
			setupWorkspace();
			repoSettingsStore.getOrCreate(REPO, "alpha");
			repoSettingsStore.update(REPO, { deleteBranchOnRemove: false });
			const { coordinator, removeWorktree, confirmRemoveWorktree } = makeCoordinator();

			await coordinator.handleRemoveWorkspace(REPO, WORKSPACE);

			expect(confirmRemoveWorktree).toHaveBeenCalledWith(WORKSPACE, SAFE_LIFECYCLE, false);
			expect(removeWorktree).toHaveBeenCalledWith(REPO, WORKSPACE, false, false);
		});
	});

	it("does not remove anything when the user cancels the confirm dialog", async () => {
		await testInScopeAsync(async () => {
			setupWorkspace();
			const { coordinator, removeWorktree, confirmRemoveWorktree } = makeCoordinator();
			confirmRemoveWorktree.mockResolvedValue(false);

			await coordinator.handleRemoveWorkspace(REPO, WORKSPACE);

			expect(removeWorktree).not.toHaveBeenCalled();
			expect(repositoriesStore.get(REPO)?.workspaces[WORKSPACE]).toBeDefined();
		});
	});

	it("asks the in-use question after the confirm dialog when a terminal is attached", async () => {
		await testInScopeAsync(async () => {
			setupWorkspace({ terminals: ["t1"] });
			const { coordinator, confirmRemoveWorktree, confirmRemoveBusyWorktree, closeTerminal } = makeCoordinator();

			await coordinator.handleRemoveWorkspace(REPO, WORKSPACE);

			expect(confirmRemoveWorktree).toHaveBeenCalled();
			expect(confirmRemoveBusyWorktree).toHaveBeenCalledWith(
				WORKSPACE,
				expect.objectContaining({ terminalCount: 1, isBusy: true }),
			);
			expect(confirmRemoveWorktree.mock.invocationCallOrder[0]).toBeLessThan(
				confirmRemoveBusyWorktree.mock.invocationCallOrder[0],
			);
			expect(confirmRemoveBusyWorktree.mock.invocationCallOrder[0]).toBeLessThan(
				closeTerminal.mock.invocationCallOrder[0],
			);
		});
	});

	it("declining the in-use question closes no terminal and removes nothing", async () => {
		await testInScopeAsync(async () => {
			setupWorkspace({ terminals: ["t1"] });
			const { coordinator, confirmRemoveBusyWorktree, closeTerminal, removeWorktree } = makeCoordinator();
			confirmRemoveBusyWorktree.mockResolvedValue(false);

			await coordinator.handleRemoveWorkspace(REPO, WORKSPACE);

			expect(closeTerminal).not.toHaveBeenCalled();
			expect(removeWorktree).not.toHaveBeenCalled();
			expect(repositoriesStore.get(REPO)?.workspaces[WORKSPACE]).toBeDefined();
		});
	});

	it("proceeds after the confirm dialog for a busy workspace when no in-use dialog is wired", async () => {
		await testInScopeAsync(async () => {
			setupWorkspace({ terminals: ["t1"] });
			const { coordinator, removeWorktree } = makeCoordinator({
				dialogs: { confirmRemoveBusyWorktree: undefined },
			});

			await coordinator.handleRemoveWorkspace(REPO, WORKSPACE);

			expect(removeWorktree).toHaveBeenCalledTimes(1);
		});
	});

	it("closes every attached terminal before invoking remove_worktree, tolerating one that fails to close", async () => {
		await testInScopeAsync(async () => {
			setupWorkspace({ terminals: ["t1", "t2"] });
			const { coordinator, closeTerminal, removeWorktree } = makeCoordinator();
			closeTerminal.mockRejectedValueOnce(new Error("pty gone"));

			await coordinator.handleRemoveWorkspace(REPO, WORKSPACE);

			expect(closeTerminal).toHaveBeenCalledWith("t1", true);
			expect(closeTerminal).toHaveBeenCalledWith("t2", true);
			expect(closeTerminal.mock.invocationCallOrder[1]).toBeLessThan(removeWorktree.mock.invocationCallOrder[0]);
			expect(repositoriesStore.get(REPO)?.workspaces[WORKSPACE]).toBeUndefined();
		});
	});

	it("asks for a separate lock override on worktree_locked, passing deleteBranch, then overrides only the lock", async () => {
		await testInScopeAsync(async () => {
			setupWorkspace();
			const { coordinator, removeWorktree, confirmRemoveLockedWorktree } = makeCoordinator();
			removeWorktree.mockRejectedValueOnce(new Error("worktree_locked:fatal: locked")).mockResolvedValueOnce({});

			await coordinator.handleRemoveWorkspace(REPO, WORKSPACE);

			expect(confirmRemoveLockedWorktree).toHaveBeenCalledWith(WORKSPACE, true);
			expect(removeWorktree).toHaveBeenLastCalledWith(REPO, WORKSPACE, true, false, true);
			expect(repositoriesStore.get(REPO)?.workspaces[WORKSPACE]).toBeUndefined();
		});
	});

	it("forces a requires_force removal only with the confirmed lifecycle fingerprint", async () => {
		await testInScopeAsync(async () => {
			setupWorkspace();
			const lifecycle = {
				dirtyFiles: 2,
				dirtyFingerprint: "fp-1",
				commitStatus: "in_sync",
				removalSafety: "requires_force",
			};
			const { coordinator, removeWorktree } = makeCoordinator({ lifecycle });

			await coordinator.handleRemoveWorkspace(REPO, WORKSPACE);

			expect(removeWorktree).toHaveBeenCalledWith(REPO, WORKSPACE, true, true, false, "fp-1");
		});
	});

	it("refuses before any dialog when the lifecycle verdict is unknown", async () => {
		await testInScopeAsync(async () => {
			setupWorkspace();
			const { coordinator, confirmRemoveWorktree, statusMessages } = makeCoordinator({
				lifecycle: { removalSafety: "unknown", error: "git failed" },
			});

			await coordinator.handleRemoveWorkspace(REPO, WORKSPACE);

			expect(confirmRemoveWorktree).not.toHaveBeenCalled();
			expect(statusMessages.some((m) => m.includes("Cannot verify"))).toBe(true);
		});
	});

	it("reports worktree_is_main without removing the workspace from the store", async () => {
		await testInScopeAsync(async () => {
			setupWorkspace();
			const { coordinator, removeWorktree, statusMessages } = makeCoordinator();
			removeWorktree.mockRejectedValueOnce(new Error("worktree_is_main:cannot remove"));

			await coordinator.handleRemoveWorkspace(REPO, WORKSPACE);

			expect(statusMessages.some((m) => m.includes("main worktree"))).toBe(true);
			expect(repositoriesStore.get(REPO)?.workspaces[WORKSPACE]).toBeDefined();
		});
	});

	it("keeps the workspace row on an unrecognized removal failure instead of silently dropping it", async () => {
		await testInScopeAsync(async () => {
			setupWorkspace();
			const { coordinator, removeWorktree, statusMessages } = makeCoordinator();
			removeWorktree.mockRejectedValueOnce(new Error("some_other_git_failure: disk full"));

			await coordinator.handleRemoveWorkspace(REPO, WORKSPACE);

			expect(statusMessages.some((m) => m.includes("Failed to remove"))).toBe(true);
			expect(repositoriesStore.get(REPO)?.workspaces[WORKSPACE]).toBeDefined();
			expect(repositoriesStore.get(REPO)?.workspaces[WORKSPACE]?.isRemoving).toBe(false);
		});
	});

	it("ignores a second concurrent call for the same workspace while the first is still in flight", async () => {
		await testInScopeAsync(async () => {
			setupWorkspace();
			let resolveConfirm!: (v: boolean) => void;
			const { coordinator, confirmRemoveWorktree, removeWorktree } = makeCoordinator();
			confirmRemoveWorktree.mockImplementation(() => new Promise<boolean>((resolve) => (resolveConfirm = resolve)));

			const first = coordinator.handleRemoveWorkspace(REPO, WORKSPACE);
			const second = coordinator.handleRemoveWorkspace(REPO, WORKSPACE);
			await vi.waitFor(() => expect(confirmRemoveWorktree).toHaveBeenCalledTimes(1));
			resolveConfirm(true);
			await Promise.all([first, second]);

			expect(confirmRemoveWorktree).toHaveBeenCalledTimes(1);
			expect(removeWorktree).toHaveBeenCalledTimes(1);
		});
	});

	it("reports 'not a worktree' and does nothing when the workspace has no worktreePath", async () => {
		await testInScopeAsync(async () => {
			repositoriesStore.add({ path: REPO, displayName: "alpha" });
			repositoriesStore.setWorkspace(REPO, "main", { worktreePath: null });
			const { coordinator, confirmRemoveWorktree, statusMessages } = makeCoordinator();

			await coordinator.handleRemoveWorkspace(REPO, "main");

			expect(confirmRemoveWorktree).not.toHaveBeenCalled();
			expect(statusMessages).toEqual(["Cannot remove main: not a worktree"]);
		});
	});

	it("keeps the branch label when the backend warns the branch survived", async () => {
		await testInScopeAsync(async () => {
			setupWorkspace();
			repoSettingsStore.getOrCreate(REPO, "alpha");
			repoSettingsStore.setLabel(REPO, WORKSPACE, "my label");
			const { coordinator, removeWorktree } = makeCoordinator();
			removeWorktree.mockResolvedValue({ branch_delete_warning: "not fully merged" });

			await coordinator.handleRemoveWorkspace(REPO, WORKSPACE);

			expect(repoSettingsStore.get(REPO)?.branchLabels[WORKSPACE]).toBe("my label");
		});
	});
});
