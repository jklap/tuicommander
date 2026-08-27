import type { Accessor, Setter } from "solid-js";
import { appLogger } from "../../stores/appLogger";
import { repoSettingsStore } from "../../stores/repoSettings";
import { repositoriesStore } from "../../stores/repositories";
import type { WorkspaceLifecycleStatus } from "../../stores/workspaceIdentity";
import { branchActivitySummary } from "../../utils/activitySnapshot";
import type { RemoveWorktreeResult } from "../useRepository";

interface WorktreeRemovalCoordinatorDeps {
	repo: {
		removeWorktree: (
			repoPath: string,
			workspaceId: string,
			deleteBranch: boolean,
			force?: boolean,
			overrideLock?: boolean,
			expectedFingerprint?: string,
			confirmMissingCheckout?: boolean,
		) => Promise<RemoveWorktreeResult | undefined>;
		getWorkspaceLifecycle: (repoPath: string, workspaceId: string) => Promise<WorkspaceLifecycleStatus>;
	};
	dialogs: {
		confirmRemoveWorktree: (
			branchName: string,
			status: WorkspaceLifecycleStatus,
			deleteBranch: boolean,
		) => Promise<boolean>;
		confirmRemoveLockedWorktree?: (branchName: string, deleteBranch?: boolean) => Promise<boolean>;
		confirmRemoveBusyWorktree?: (
			branchName: string,
			summary: ReturnType<typeof branchActivitySummary>,
		) => Promise<boolean>;
	};
	closeTerminal: (id: string, skipConfirm?: boolean) => Promise<void>;
	setStatusInfo: (message: string) => void;
	removingBranches: Accessor<Set<string>>;
	setRemovingBranches: Setter<Set<string>>;
}

function describeRemoveWorktreeSuccess(branchName: string, outcome: RemoveWorktreeResult | undefined): string {
	if (outcome?.branch_delete_warning) {
		return `Removed ${branchName} worktree; branch was kept: ${outcome.branch_delete_warning}`;
	}
	return `Removed ${branchName}`;
}

/** Owns worktree removal locking, force recovery, and store cleanup. */
export function createWorktreeRemovalCoordinator(deps: WorktreeRemovalCoordinatorDeps) {
	const { removingBranches, setRemovingBranches } = deps;

	/** `workspaceId` addresses the row and the backend. `branchName`, read off the
	 *  record, is what the user sees in the confirm dialog and the status line —
	 *  showing a minted id there would be showing a user an internal key. */
	const handleRemoveWorkspace = async (repoPath: string, workspaceId: string) => {
		const removeKey = `${repoPath}::${workspaceId}`;
		// Lock IMMEDIATELY (synchronously) to prevent concurrent invocations that race the awaits below
		if (removingBranches().has(removeKey)) return;
		setRemovingBranches((prev) => new Set([...prev, removeKey]));

		const clearLock = () => {
			setRemovingBranches((prev) => {
				const next = new Set(prev);
				next.delete(removeKey);
				return next;
			});
		};

		const repoState = repositoriesStore.get(repoPath);
		const branch = repoState?.workspaces[workspaceId];
		if (!branch?.worktreePath) {
			deps.setStatusInfo(`Cannot remove ${workspaceId}: not a worktree`);
			clearLock();
			return;
		}
		const branchName = branch.branchName;

		const effective = repoSettingsStore.getEffective(repoPath);
		const deleteBranch = effective?.deleteBranchOnRemove ?? true;
		let lifecycle: WorkspaceLifecycleStatus;
		try {
			lifecycle = await deps.repo.getWorkspaceLifecycle(repoPath, workspaceId);
		} catch (err) {
			appLogger.warn("git", `workspace lifecycle preflight failed for ${workspaceId}`, err);
			deps.setStatusInfo(`Cannot verify whether ${branchName} is safe to remove`);
			clearLock();
			return;
		}
		if (lifecycle.removalSafety === "unknown") {
			deps.setStatusInfo(
				`Cannot verify whether ${branchName} is safe to remove: ${lifecycle.error ?? "unknown state"}`,
			);
			clearLock();
			return;
		}
		if (deleteBranch && lifecycle.commitStatus === "unmerged") {
			deps.setStatusInfo(
				`Cannot remove ${branchName}: the branch has unmerged commits. Merge it first, or turn off Delete branch on remove.`,
			);
			clearLock();
			return;
		}

		const confirmed = await deps.dialogs.confirmRemoveWorktree(branchName, lifecycle, deleteBranch);
		if (!confirmed) {
			clearLock();
			return;
		}

		// Computed from the workspace's terminal list as of THIS click, before
		// anything is closed: a workspace with a live (even idle) terminal gets a
		// second, Cancel-by-default dialog that says so, BEFORE the close-terminal
		// loop below ever runs (2026-08-26 incident).
		const activity = branchActivitySummary(branch.terminals);
		if (activity.isBusy && deps.dialogs.confirmRemoveBusyWorktree) {
			let busyConfirmed = false;
			try {
				busyConfirmed = await deps.dialogs.confirmRemoveBusyWorktree(branchName, activity);
			} catch (dialogErr) {
				appLogger.error("git", `handleRemoveWorkspace: confirmRemoveBusyWorktree threw`, {
					workspaceId,
					error: dialogErr instanceof Error ? dialogErr.message : String(dialogErr),
				});
			}
			if (!busyConfirmed) {
				appLogger.info("git", `handleRemoveWorkspace: user cancelled removal of a workspace in use`, { workspaceId });
				clearLock();
				return;
			}
		}

		// Show "Removing…" in sidebar as soon as the user confirms — before
		// the terminal-close loop, which can take noticeable time. Otherwise
		// the lock is held while the UI still appears clickable.
		repositoriesStore.setWorkspace(repoPath, workspaceId, { isRemoving: true });

		// Close terminals defensively: a thrown error here used to leak the
		// removingBranches lock (clearLock was unreachable) and left isRemoving
		// stuck. Catch per-terminal so one bad PTY doesn't block cleanup.
		for (const termId of branch.terminals) {
			try {
				await deps.closeTerminal(termId, true);
			} catch (err) {
				appLogger.warn("git", `handleRemoveWorkspace: closeTerminal failed`, {
					termId,
					workspaceId,
					error: err instanceof Error ? err.message : String(err),
				});
			}
		}

		appLogger.info("git", `handleRemoveWorkspace: invoking remove_worktree`, {
			repoPath,
			workspaceId,
			worktreePath: branch.worktreePath,
			deleteBranch,
		});

		// Tracks whether to remove the branch from the store at the end.
		// Set to true on success or non-fatal non-lock errors (old "remove from UI" behavior).
		// Stays false when: locked+cancelled, or force-remove failed (worktree still in git).
		let shouldRemoveFromStore = false;
		let shouldClearBranchLabel = true;
		const removeConfirmed = (overrideLock: boolean) => {
			if (lifecycle.removalSafety === "requires_force") {
				if (lifecycle.missingCheckout) {
					return deps.repo.removeWorktree(repoPath, workspaceId, deleteBranch, true, overrideLock, undefined, true);
				}
				if (!lifecycle.dirtyFingerprint) {
					throw new Error("Cannot verify the confirmed worktree state");
				}
				return deps.repo.removeWorktree(
					repoPath,
					workspaceId,
					deleteBranch,
					true,
					overrideLock,
					lifecycle.dirtyFingerprint,
				);
			}
			return overrideLock
				? deps.repo.removeWorktree(repoPath, workspaceId, deleteBranch, false, true)
				: deps.repo.removeWorktree(repoPath, workspaceId, deleteBranch, false);
		};
		try {
			// The user confirmed knowing the count, so the backend guard would only
			// bounce a decision that has already been made.
			const outcome = await removeConfirmed(false);
			appLogger.info("git", `handleRemoveWorkspace: remove_worktree SUCCESS`, { workspaceId });
			shouldRemoveFromStore = true;
			shouldClearBranchLabel = !outcome?.branch_delete_warning;
			deps.setStatusInfo(describeRemoveWorktreeSuccess(branchName, outcome));
		} catch (err) {
			const reason = err instanceof Error ? err.message : String(err);
			if (reason.startsWith("worktree_locked:")) {
				// Worktree is locked by an agent — ask for a separate lock override.
				repositoriesStore.setWorkspace(repoPath, workspaceId, { isRemoving: false });
				appLogger.warn("git", `handleRemoveWorkspace: worktree locked — showing confirmation dialog`, {
					workspaceId,
					reason,
				});
				// Pass deleteBranch so the dialog can describe the requested cleanup.
				// Catch dialog rejection so the removingBranches
				// lock is released even when the modal subsystem errors out.
				let forceConfirmed = false;
				try {
					forceConfirmed = await (deps.dialogs.confirmRemoveLockedWorktree?.(branchName, deleteBranch) ?? false);
				} catch (dialogErr) {
					appLogger.error("git", `handleRemoveWorkspace: confirmRemoveLockedWorktree threw`, {
						workspaceId,
						error: dialogErr instanceof Error ? dialogErr.message : String(dialogErr),
					});
					deps.setStatusInfo(`Failed to confirm force-remove for ${branchName}`);
					clearLock();
					return;
				}
				if (!forceConfirmed) {
					appLogger.info("git", `handleRemoveWorkspace: user cancelled force removal of locked worktree`, {
						workspaceId,
					});
					clearLock();
					return;
				}
				repositoriesStore.setWorkspace(repoPath, workspaceId, { isRemoving: true });
				try {
					const outcome = await removeConfirmed(true);
					appLogger.info("git", `handleRemoveWorkspace: force remove_worktree SUCCESS`, { workspaceId });
					shouldRemoveFromStore = true;
					shouldClearBranchLabel = !outcome?.branch_delete_warning;
					deps.setStatusInfo(describeRemoveWorktreeSuccess(branchName, outcome));
				} catch (forceErr) {
					const forceReason = forceErr instanceof Error ? forceErr.message : String(forceErr);
					appLogger.error("git", `handleRemoveWorkspace: force remove_worktree FAILED`, {
						workspaceId,
						reason: forceReason,
					});
					deps.setStatusInfo(`Failed to remove ${branchName}: ${forceReason}`);
					repositoriesStore.setWorkspace(repoPath, workspaceId, { isRemoving: false });
					clearLock();
					return;
				}
			} else if (reason.startsWith("worktree_is_main:")) {
				appLogger.warn("git", `handleRemoveWorkspace: branch is in main worktree — cannot remove as worktree`, {
					workspaceId,
				});
				deps.setStatusInfo(`Cannot remove ${branchName}: branch is in the main worktree, not a linked worktree`);
				repositoriesStore.setWorkspace(repoPath, workspaceId, { isRemoving: false });
				clearLock();
				return;
			} else {
				appLogger.error("git", `handleRemoveWorkspace: remove_worktree FAILED — workspace kept`, {
					workspaceId,
					reason,
				});
				deps.setStatusInfo(`Failed to remove ${branchName}: ${reason}`);
				repositoriesStore.setWorkspace(repoPath, workspaceId, { isRemoving: false });
				clearLock();
				return;
			}
		}

		if (!shouldRemoveFromStore) {
			repositoriesStore.setWorkspace(repoPath, workspaceId, { isRemoving: false });
			clearLock();
			return;
		}
		appLogger.info("git", `handleRemoveWorkspace: calling removeWorkspace on store`, { workspaceId });
		clearLock();
		repositoriesStore.removeWorkspace(repoPath, workspaceId);
		if (shouldClearBranchLabel) {
			repoSettingsStore.setLabel(repoPath, branchName, null);
		}
	};

	return { handleRemoveWorkspace };
}
