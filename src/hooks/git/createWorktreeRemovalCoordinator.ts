import type { Accessor, Setter } from "solid-js";
import { appLogger } from "../../stores/appLogger";
import { repoSettingsStore } from "../../stores/repoSettings";
import { repositoriesStore } from "../../stores/repositories";
import { branchActivitySummary } from "../../utils/activitySnapshot";
import type { RemoveWorktreeResult } from "../useRepository";

interface WorktreeRemovalCoordinatorDeps {
	repo: {
		removeWorktree: (
			repoPath: string,
			branchName: string,
			deleteBranch: boolean,
			force?: boolean,
			overrideBusy?: boolean,
		) => Promise<RemoveWorktreeResult | undefined>;
	};
	dialogs: {
		confirmRemoveWorktree: (branchName: string) => Promise<boolean>;
		confirmRemoveLockedWorktree?: (branchName: string, deleteBranch?: boolean, isDirty?: boolean) => Promise<boolean>;
		confirmRemoveBusyWorktree?: (
			branchName: string,
			summary: ReturnType<typeof branchActivitySummary>,
		) => Promise<boolean>;
		confirmForceRemoveDirtyWorktree?: (branchName: string) => Promise<boolean>;
	};
	/** `null` when the backend can't answer — treated as "don't know," not clean. */
	checkWorktreeDirty: (repoPath: string, branchName: string) => Promise<boolean | null>;
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

	const handleRemoveBranch = async (repoPath: string, branchName: string) => {
		const removeKey = `${repoPath}::${branchName}`;
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
		const branch = repoState?.branches[branchName];
		if (!branch?.worktreePath) {
			deps.setStatusInfo(`Cannot remove ${branchName}: not a worktree`);
			clearLock();
			return;
		}

		// Computed from `branch.terminals` — the terminal list as of THIS click,
		// before anything is closed. This is what stops the class of incident
		// where a worktree with a live (even idle) terminal attached got deleted:
		// a busy branch gets a dialog that says so, BEFORE the close-terminal
		// loop below ever runs. See plans/worktree-removal-incident-2026-08-26.md.
		const activity = branchActivitySummary(branch.terminals);
		const confirmed = activity.isBusy
			? await (deps.dialogs.confirmRemoveBusyWorktree?.(branchName, activity) ??
					deps.dialogs.confirmRemoveWorktree(branchName))
			: await deps.dialogs.confirmRemoveWorktree(branchName);
		if (!confirmed) {
			clearLock();
			return;
		}

		// Show "Removing…" in sidebar as soon as the user confirms — before
		// the terminal-close loop, which can take noticeable time. Otherwise
		// the lock is held while the UI still appears clickable.
		repositoriesStore.setBranch(repoPath, branchName, { isRemoving: true });

		// Close terminals defensively: a thrown error here used to leak the
		// removingBranches lock (clearLock was unreachable) and left isRemoving
		// stuck. Catch per-terminal so one bad PTY doesn't block cleanup.
		for (const termId of branch.terminals) {
			try {
				await deps.closeTerminal(termId, true);
			} catch (err) {
				appLogger.warn("git", `handleRemoveBranch: closeTerminal failed`, {
					termId,
					branchName,
					error: err instanceof Error ? err.message : String(err),
				});
			}
		}

		const effective = repoSettingsStore.getEffective(repoPath);
		const deleteBranch = effective?.deleteBranchOnRemove ?? true;
		appLogger.info("git", `handleRemoveBranch: invoking remove_worktree`, {
			repoPath,
			branchName,
			worktreePath: branch.worktreePath,
			deleteBranch,
		});

		// Tracks whether to remove the branch from the store at the end.
		// Set to true on success only — an unrecognized failure now KEEPS the
		// branch row (see the final `else` arm below). It used to remove the row
		// unconditionally on any non-lock error ("remove from UI only"), which
		// silently hid a worktree that was still on disk the moment `worktree_dirty:`
		// became a real refusal instead of being masked by the removed stray
		// `--force` (2026-08-26 incident, root cause #1/#4 in the writeup).
		let shouldRemoveFromStore = false;
		let shouldClearBranchLabel = true;
		try {
			// First attempt never overrides anything: Safe mode, no busy override.
			// The terminal-close loop above already released everything THIS
			// instance knew about, so this fails only when git itself refuses
			// (dirty/locked) or another session neither the frontend nor this
			// close loop knew about is still attached.
			const outcome = await deps.repo.removeWorktree(repoPath, branchName, deleteBranch);
			appLogger.info("git", `handleRemoveBranch: remove_worktree SUCCESS`, { branchName });
			shouldRemoveFromStore = true;
			shouldClearBranchLabel = !outcome?.branch_delete_warning;
			deps.setStatusInfo(describeRemoveWorktreeSuccess(branchName, outcome));
		} catch (err) {
			const reason = err instanceof Error ? err.message : String(err);
			if (reason.startsWith("worktree_busy:")) {
				// A session is attached that this instance's terminal-close loop
				// didn't reach — e.g. an MCP/HTTP-spawned session, or one that
				// attached in the race between the confirm dialog and this call.
				// This is the backend's own backstop; ask again, now with what the
				// backend actually found attached.
				repositoriesStore.setBranch(repoPath, branchName, { isRemoving: false });
				appLogger.warn("git", `handleRemoveBranch: worktree busy — showing confirmation dialog`, {
					branchName,
					reason,
				});
				let overrideConfirmed = false;
				try {
					overrideConfirmed = await (deps.dialogs.confirmRemoveBusyWorktree?.(branchName, activity) ?? false);
				} catch (dialogErr) {
					appLogger.error("git", `handleRemoveBranch: confirmRemoveBusyWorktree threw`, {
						branchName,
						error: dialogErr instanceof Error ? dialogErr.message : String(dialogErr),
					});
					deps.setStatusInfo(`Failed to confirm busy-override for ${branchName}`);
					clearLock();
					return;
				}
				if (!overrideConfirmed) {
					appLogger.info("git", `handleRemoveBranch: user cancelled busy-override removal`, { branchName });
					clearLock();
					return;
				}
				repositoriesStore.setBranch(repoPath, branchName, { isRemoving: true });
				try {
					const outcome = await deps.repo.removeWorktree(repoPath, branchName, deleteBranch, false, true);
					appLogger.info("git", `handleRemoveBranch: override-busy remove_worktree SUCCESS`, { branchName });
					shouldRemoveFromStore = true;
					shouldClearBranchLabel = !outcome?.branch_delete_warning;
					deps.setStatusInfo(describeRemoveWorktreeSuccess(branchName, outcome));
				} catch (overrideErr) {
					const overrideReason = overrideErr instanceof Error ? overrideErr.message : String(overrideErr);
					appLogger.error("git", `handleRemoveBranch: override-busy remove_worktree FAILED`, {
						branchName,
						reason: overrideReason,
					});
					deps.setStatusInfo(`Failed to remove ${branchName}: ${overrideReason}`);
					repositoriesStore.setBranch(repoPath, branchName, { isRemoving: false });
					clearLock();
					return;
				}
			} else if (reason.startsWith("worktree_dirty:")) {
				// Safe mode was refused because the worktree has uncommitted work.
				// `overrideBusy` is not involved here — this is purely the
				// dirty-file confirmation, independent of whether anything is
				// attached (root cause #1: the old code silently discarded
				// uncommitted work here instead of asking).
				repositoriesStore.setBranch(repoPath, branchName, { isRemoving: false });
				appLogger.warn("git", `handleRemoveBranch: worktree dirty — showing confirmation dialog`, {
					branchName,
					reason,
				});
				let forceConfirmed = false;
				try {
					forceConfirmed = await (deps.dialogs.confirmForceRemoveDirtyWorktree?.(branchName) ?? false);
				} catch (dialogErr) {
					appLogger.error("git", `handleRemoveBranch: confirmForceRemoveDirtyWorktree threw`, {
						branchName,
						error: dialogErr instanceof Error ? dialogErr.message : String(dialogErr),
					});
					deps.setStatusInfo(`Failed to confirm force-remove for ${branchName}`);
					clearLock();
					return;
				}
				if (!forceConfirmed) {
					appLogger.info("git", `handleRemoveBranch: user cancelled dirty-worktree removal`, { branchName });
					clearLock();
					return;
				}
				repositoriesStore.setBranch(repoPath, branchName, { isRemoving: true });
				try {
					const outcome = await deps.repo.removeWorktree(repoPath, branchName, deleteBranch, true);
					appLogger.info("git", `handleRemoveBranch: force remove_worktree SUCCESS`, { branchName });
					shouldRemoveFromStore = true;
					shouldClearBranchLabel = !outcome?.branch_delete_warning;
					deps.setStatusInfo(describeRemoveWorktreeSuccess(branchName, outcome));
				} catch (forceErr) {
					const forceReason = forceErr instanceof Error ? forceErr.message : String(forceErr);
					appLogger.error("git", `handleRemoveBranch: force remove_worktree FAILED`, {
						branchName,
						reason: forceReason,
					});
					deps.setStatusInfo(`Failed to remove ${branchName}: ${forceReason}`);
					repositoriesStore.setBranch(repoPath, branchName, { isRemoving: false });
					clearLock();
					return;
				}
			} else if (reason.startsWith("worktree_locked:")) {
				// Worktree is locked (git-level lock, taken on session attach — see
				// `lock_worktree_for_session`) — ask user to confirm force removal.
				repositoriesStore.setBranch(repoPath, branchName, { isRemoving: false });
				appLogger.warn("git", `handleRemoveBranch: worktree locked — showing confirmation dialog`, {
					branchName,
					reason,
				});
				// Force-removing a locked worktree uses Forced (double --force),
				// which overrides a dirty-worktree refusal too — so confirming
				// "Force Remove" here can silently discard uncommitted work with
				// no warning of its own. Check dirtiness so the dialog can say so.
				// An unanswered check (`null`) is treated as possibly dirty —
				// fail toward warning, not toward silence.
				const isDirty = (await deps.checkWorktreeDirty(repoPath, branchName)) !== false;
				// Catch dialog rejection so the removingBranches lock is released
				// even when the modal subsystem errors out.
				let forceConfirmed = false;
				try {
					forceConfirmed = await (deps.dialogs.confirmRemoveLockedWorktree?.(branchName, deleteBranch, isDirty) ??
						false);
				} catch (dialogErr) {
					appLogger.error("git", `handleRemoveBranch: confirmRemoveLockedWorktree threw`, {
						branchName,
						error: dialogErr instanceof Error ? dialogErr.message : String(dialogErr),
					});
					deps.setStatusInfo(`Failed to confirm force-remove for ${branchName}`);
					clearLock();
					return;
				}
				if (!forceConfirmed) {
					appLogger.info("git", `handleRemoveBranch: user cancelled force removal of locked worktree`, { branchName });
					clearLock();
					return;
				}
				repositoriesStore.setBranch(repoPath, branchName, { isRemoving: true });
				try {
					const outcome = await deps.repo.removeWorktree(repoPath, branchName, deleteBranch, true);
					appLogger.info("git", `handleRemoveBranch: force remove_worktree SUCCESS`, { branchName });
					shouldRemoveFromStore = true;
					shouldClearBranchLabel = !outcome?.branch_delete_warning;
					deps.setStatusInfo(describeRemoveWorktreeSuccess(branchName, outcome));
				} catch (forceErr) {
					const forceReason = forceErr instanceof Error ? forceErr.message : String(forceErr);
					appLogger.error("git", `handleRemoveBranch: force remove_worktree FAILED`, {
						branchName,
						reason: forceReason,
					});
					deps.setStatusInfo(`Failed to remove ${branchName}: ${forceReason}`);
					repositoriesStore.setBranch(repoPath, branchName, { isRemoving: false });
					clearLock();
					return;
				}
			} else if (reason.startsWith("worktree_is_main:")) {
				appLogger.warn("git", `handleRemoveBranch: branch is in main worktree — cannot remove as worktree`, {
					branchName,
				});
				deps.setStatusInfo(`Cannot remove ${branchName}: branch is in the main worktree, not a linked worktree`);
				repositoriesStore.setBranch(repoPath, branchName, { isRemoving: false });
				clearLock();
				return;
			} else {
				// Unrecognized failure: the worktree is presumably still on disk.
				// Keep the sidebar row and report the failure instead of silently
				// dropping it — see the comment on `shouldRemoveFromStore` above.
				appLogger.error("git", `handleRemoveBranch: remove_worktree FAILED`, {
					branchName,
					reason,
				});
				deps.setStatusInfo(`Failed to remove ${branchName}: ${reason}`);
				repositoriesStore.setBranch(repoPath, branchName, { isRemoving: false });
				clearLock();
				return;
			}
		}

		if (!shouldRemoveFromStore) {
			repositoriesStore.setBranch(repoPath, branchName, { isRemoving: false });
			clearLock();
			return;
		}
		appLogger.info("git", `handleRemoveBranch: calling removeBranch on store`, { branchName });
		clearLock();
		repositoriesStore.removeBranch(repoPath, branchName);
		if (shouldClearBranchLabel) {
			repoSettingsStore.setLabel(repoPath, branchName, null);
		}
	};

	return { handleRemoveBranch };
}
