import { repositoriesStore } from "../stores/repositories";

/** Dependencies injected into useQuickSwitcher */
export interface QuickSwitcherDeps {
	handleBranchSelect: (repoPath: string, branchName: string) => void;
}

/** Quick switcher: resolve shortcut index to repo+branch.
 * Must use the same repo ordering as Sidebar.repoShortcutStarts:
 * grouped repos (in groupOrder) first, then ungrouped repos.
 * Only counts branches that are actually visible (skips collapsed repos/groups). */
export function useQuickSwitcher(deps: QuickSwitcherDeps) {
	const switchToBranchByIndex = (index: number) => {
		let counter = 1;
		const layout = repositoriesStore.getGroupedLayout();

		// Collect visible repos: skip collapsed groups, skip collapsed/non-expanded repos
		const visibleRepos: typeof layout.ungrouped = [];
		for (const entry of layout.groups) {
			if (entry.group.collapsed) continue;
			for (const repo of entry.repos) {
				if (repo.expanded && !repo.collapsed) visibleRepos.push(repo);
			}
		}
		for (const repo of layout.ungrouped) {
			if (repo.expanded && !repo.collapsed) visibleRepos.push(repo);
		}

		for (const repo of visibleRepos) {
			// Sorted for display by branch, selected by KEY: `handleBranchSelect`
			// indexes the workspace map, and passing `branchName` there picked
			// whichever row happened to be keyed by the bare branch.
			const workspaces = Object.entries(repo.workspaces).sort(([, a], [, b]) => {
				if (a.isMain && !b.isMain) return -1;
				if (!a.isMain && b.isMain) return 1;
				return a.branchName.localeCompare(b.branchName);
			});
			for (const [workspaceId] of workspaces) {
				if (counter === index) {
					deps.handleBranchSelect(repo.path, workspaceId);
					return;
				}
				counter++;
			}
		}
	};

	return {
		switchToBranchByIndex,
	};
}
