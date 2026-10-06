import { syncScopeForActiveRepo } from "../hooks/useWorktreeConsolidation";
import { globalWorkspaceStore } from "../stores/globalWorkspace";
import { paneLayoutStore } from "../stores/paneLayout";
import { repositoriesStore } from "../stores/repositories";
import { terminalsStore } from "../stores/terminals";
import { resolvePaneLayoutForBranch, savePaneLayoutForBranch } from "./branchPaneLayout";
import { filterValidTerminals } from "./terminalFilter";

/**
 * Navigate to a terminal: switch repo/branch context, activate the terminal,
 * deactivate other tab stores, activate the correct pane group, and focus.
 *
 * Clicking an ordinary sidebar terminal row is the *only* way to leave the
 * manual Global Workspace view (the sidebar pill's own click is one-way — see
 * `globalWorkspaceStore.isManualWorkspaceActive`'s doc comment) — this always
 * deactivates, even for a terminal that happens to already be promoted into
 * the workspace, since a sidebar row click is a deliberate "go look at this
 * repo/branch" action, not a within-workspace tab switch. If it's currently
 * showing, exit it first, then re-assert whatever's correct for the
 * now-active repo (its own consolidated view, or nothing) — needed even when
 * the repo doesn't actually change, since Solid's reactive effect for that
 * won't re-fire on an unchanged value.
 *
 * NOT used for the main TabBar's own tab clicks while the workspace's tab
 * strip is showing — `useTerminalLifecycle.ts`'s `handleTerminalSelect`
 * special-cases that (a tab already in `globalWorkspaceStore.getPromotedIds()`)
 * to just switch the active tab, bypassing this function's deactivate.
 */
export function navigateToTerminal(id: string): void {
	if (globalWorkspaceStore.isManualWorkspaceActive()) {
		globalWorkspaceStore.deactivate();
	}

	const repoPath = repositoriesStore.getRepoPathForTerminal(id);
	if (repoPath) {
		const repo = repositoriesStore.state.repositories[repoPath];
		if (repo) {
			for (const [workspaceId, workspace] of Object.entries(repo.workspaces)) {
				if (workspace.terminals.includes(id)) {
					const repoChanges = repositoriesStore.state.activeRepoPath !== repoPath;
					const workspaceChanges = repo.activeWorkspaceId !== workspaceId;
					if (repoChanges || workspaceChanges) {
						// Same save-on-leave / resolve-on-arrive a branch-row select does. Without
						// it the branch being left never gets its split saved, and the first
						// reset while away (any branch select) destroys it for good.
						const prevRepoPath = repositoriesStore.state.activeRepoPath;
						const prevWorkspaceId = prevRepoPath
							? repositoriesStore.state.repositories[prevRepoPath]?.activeWorkspaceId
							: null;
						if (prevRepoPath && prevWorkspaceId) savePaneLayoutForBranch(prevRepoPath, prevWorkspaceId);
						if (repoChanges) repositoriesStore.setActive(repoPath);
						if (workspaceChanges) repositoriesStore.setActiveWorkspace(repoPath, workspaceId);
						const validTerminals = filterValidTerminals(workspace.terminals, terminalsStore.getIds()).filter(
							(tid) => !terminalsStore.isDetached(tid),
						);
						resolvePaneLayoutForBranch(repoPath, workspaceId, validTerminals);
					}
					break;
				}
			}
		}
	}
	syncScopeForActiveRepo();

	// setActive deactivates the diff/markdown/editor panes itself.
	terminalsStore.setActive(id);

	if (paneLayoutStore.isSplit()) {
		const groupId = paneLayoutStore.getGroupForTab(id);
		if (groupId) {
			paneLayoutStore.setActiveGroup(groupId);
			paneLayoutStore.setActiveTab(groupId, id);
		}
	}

	requestAnimationFrame(() => terminalsStore.get(id)?.ref?.focus());
}
