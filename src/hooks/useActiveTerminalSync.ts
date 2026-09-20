import { createEffect, on } from "solid-js";
import { activityStore } from "../stores/activityStore";
import { repositoriesStore } from "../stores/repositories";
import { terminalsStore } from "../stores/terminals";

/** Synchronizes active-terminal context with activity and repository state. */
export function useActiveTerminalSync(): void {
	createEffect(
		on(
			() => terminalsStore.state.activeId,
			(id) => {
				if (!id) return;
				activityStore.dismissItem(`terminal-done-${id}`);
			},
			{ defer: true },
		),
	);

	createEffect(
		on(
			() => terminalsStore.state.activeId,
			(id) => {
				if (!id) return;
				const repoPath = repositoriesStore.getRepoPathForTerminal(id);
				if (!repoPath) return;
				const repo = repositoriesStore.state.repositories[repoPath];
				if (!repo) return;
				for (const [branchName, branch] of Object.entries(repo.workspaces)) {
					if (!branch.terminals.includes(id)) continue;
					if (branch.lastActiveTerminal !== id) {
						repositoriesStore.setWorkspace(repoPath, branchName, { lastActiveTerminal: id });
					}
					break;
				}
			},
			{ defer: true },
		),
	);
}
