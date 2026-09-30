import { createEffect } from "solid-js";
import { globalWorkspaceStore, MANUAL_SCOPE } from "../stores/globalWorkspace";
import { repoSettingsStore } from "../stores/repoSettings";
import { repositoriesStore } from "../stores/repositories";
import { terminalsStore } from "../stores/terminals";

/**
 * Terminal ids of every worktree of `repoPath`, in branch order — filtered to
 * ones that are actually still live in `terminalsStore`.
 *
 * The main branch is excluded: it has no worktree directory (`worktreePath` is
 * null), so consolidating it would drop the repo's ordinary terminals into the
 * worktree view.
 *
 * `branch.terminals` is deliberately never pruned when a terminal's process
 * exits (only when its tab is closed — see `src/AGENTS.md`'s "branch.terminals
 * Membership Must Never Be Pruned On Terminal Exit"), so a dead worktree
 * terminal's id can sit there indefinitely. Feeding that stale id straight into
 * `syncScopeMembers` as "wanted" would resurrect it into the repo's
 * auto-consolidation scope on every reactive re-run, even after
 * `onTerminalRemoved` already correctly swept it out — this filter is what
 * stops that resurrection at the source, without touching `branch.terminals`
 * itself (which other consumers still need the unpruned form of).
 */
export function worktreeTerminalsOf(repoPath: string): string[] {
	const repo = repositoriesStore.state.repositories[repoPath];
	if (!repo) return [];
	return Object.values(repo.workspaces)
		.filter((branch) => branch.worktreePath !== null)
		.flatMap((branch) => branch.terminals)
		.filter((id) => terminalsStore.get(id) !== undefined);
}

/** Repos whose per-repo consolidation toggle is on. */
export function consolidatedRepos(): string[] {
	return Object.values(repoSettingsStore.state.settings)
		.filter((settings) => settings.autoConsolidateWorktrees)
		.map((settings) => settings.path);
}

/**
 * Show the consolidated view for whichever repo is currently active if it has
 * worktree-consolidation on, and get out of the way (back to `MANUAL_SCOPE`) if
 * not.
 *
 * Exported (not just an effect body) so a caller that just exited the manual
 * Global Workspace via an imperative `deactivate()` — `navigateToTerminal.ts`,
 * `createBranchSelectionCoordinator.ts` — can re-assert the correct scope for
 * the active repo right away. Solid's `createEffect` below only re-fires on an
 * actual change to `activeRepoPath`; clicking a terminal within the *same*
 * already-active repo won't change that value, so the reactive trigger alone
 * can't be relied on after an imperative exit.
 */
export function syncScopeForActiveRepo(): void {
	const repoPath = repositoriesStore.state.activeRepoPath;
	const enabled = repoPath ? (repoSettingsStore.state.settings[repoPath]?.autoConsolidateWorktrees ?? false) : false;

	if (enabled && repoPath) {
		globalWorkspaceStore.setScope(repoPath);
		if (!globalWorkspaceStore.isActive() && globalWorkspaceStore.hasPromoted()) {
			globalWorkspaceStore.activate();
		}
		return;
	}
	if (globalWorkspaceStore.getScope() !== MANUAL_SCOPE) {
		globalWorkspaceStore.deactivate();
		globalWorkspaceStore.setScope(MANUAL_SCOPE);
	}
}

/**
 * Keep each consolidated repo's workspace in sync with its worktrees (#e767).
 *
 * Declarative rather than event-driven: the effect recomputes the full member
 * list from the repositories store, so a worktree created, removed or archived
 * needs no dedicated hook — the store write that adds or drops its terminals is
 * the trigger, and `syncScopeMembers` is idempotent.
 *
 * Every enabled repo is synced, not just the active one, so switching to a
 * consolidated repo shows a view that is already correct instead of one that
 * assembles itself after the fact.
 */
export function useWorktreeConsolidation(): void {
	createEffect(() => {
		for (const repoPath of consolidatedRepos()) {
			globalWorkspaceStore.syncScopeMembers(repoPath, worktreeTerminalsOf(repoPath));
		}
	});

	// The MANUAL_SCOPE guard inside syncScopeForActiveRepo is what keeps this
	// from closing a workspace the user promoted by hand. syncScopeForActiveRepo
	// itself reads activeRepoPath/autoConsolidateWorktrees, so calling it here
	// is enough to establish this effect's reactive dependency on both.
	createEffect(syncScopeForActiveRepo);
}
