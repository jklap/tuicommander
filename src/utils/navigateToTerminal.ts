import { syncScopeForActiveRepo } from "../hooks/useWorktreeConsolidation";
import { globalWorkspaceStore } from "../stores/globalWorkspace";
import { paneLayoutStore } from "../stores/paneLayout";
import { repositoriesStore } from "../stores/repositories";
import { terminalsStore } from "../stores/terminals";

/**
 * Navigate to a terminal: switch repo/branch context, activate the terminal,
 * deactivate other tab stores, activate the correct pane group, and focus.
 *
 * Clicking an ordinary sidebar terminal row is the *only* way to leave the
 * manual Global Workspace view (the sidebar pill's own click is one-way — see
 * `globalWorkspaceStore.isManualWorkspaceActive`'s doc comment). If it's
 * currently showing, exit it first, then re-assert whatever's correct for the
 * now-active repo (its own consolidated view, or nothing) — needed even when
 * the repo doesn't actually change, since Solid's reactive effect for that
 * won't re-fire on an unchanged value.
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
