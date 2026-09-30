import { syncScopeForActiveRepo } from "../hooks/useWorktreeConsolidation";
import { globalWorkspaceStore, MANUAL_SCOPE } from "../stores/globalWorkspace";
import { paneLayoutStore } from "../stores/paneLayout";
import { repositoriesStore } from "../stores/repositories";
import { paneLayoutKey } from "../stores/savedPaneLayouts";
import { terminalsStore } from "../stores/terminals";

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
	if (!terminalsStore.get(id)) return;

	// Unowned sessions are parked in the manual workspace, not in the selected
	// repo. Open that tab strip before selecting the terminal's content.
	const repoPath = repositoriesStore.getRepoPathForTerminal(id);
	if (!repoPath && globalWorkspaceStore.getScopeMembers(MANUAL_SCOPE).includes(id)) {
		const activeRepo = repositoriesStore.getActive();
		const layoutKey = activeRepo?.activeWorkspaceId
			? paneLayoutKey(activeRepo.path, activeRepo.activeWorkspaceId)
			: undefined;
		if (globalWorkspaceStore.getScope() !== MANUAL_SCOPE) {
			globalWorkspaceStore.deactivate();
			// Leave repo consolidation before opening the repo-independent workspace.
			repositoriesStore.setActive(null);
			globalWorkspaceStore.setScope(MANUAL_SCOPE);
		}
		globalWorkspaceStore.activate(layoutKey);
	} else if (globalWorkspaceStore.isManualWorkspaceActive()) {
		// A sidebar row click always leaves the manual Global Workspace, even
		// for a terminal already promoted into it (see the doc comment above).
		globalWorkspaceStore.deactivate();
	} else if (globalWorkspaceStore.isActive() && !globalWorkspaceStore.getPromotedIds().includes(id)) {
		// A repo's own auto-consolidated view: leave it only for a terminal it
		// does not show (ambient scope, like the TabBar's tab strip).
		globalWorkspaceStore.deactivate();
	}
	if (repoPath) {
		const repo = repositoriesStore.state.repositories[repoPath];
		if (repo) {
			for (const [workspaceId, workspace] of Object.entries(repo.workspaces)) {
				if (workspace.terminals.includes(id)) {
					if (repositoriesStore.state.activeRepoPath !== repoPath) {
						repositoriesStore.setActive(repoPath);
					}
					if (repo.activeWorkspaceId !== workspaceId) {
						repositoriesStore.setActiveWorkspace(repoPath, workspaceId);
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
