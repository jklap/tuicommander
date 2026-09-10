import { paneLayoutStore } from "../stores/paneLayout";
import { repositoriesStore } from "../stores/repositories";
import { terminalsStore } from "../stores/terminals";

/**
 * Navigate to a terminal: switch repo/branch context, activate the terminal,
 * deactivate other tab stores, activate the correct pane group, and focus.
 */
export function navigateToTerminal(id: string): void {
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
