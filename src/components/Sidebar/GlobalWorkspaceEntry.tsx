import { type Component, Show } from "solid-js";
import { globalWorkspaceStore, MANUAL_SCOPE } from "../../stores/globalWorkspace";
import { repositoriesStore } from "../../stores/repositories";
import { paneLayoutKey } from "../../stores/savedPaneLayouts";
import { GlobeIcon } from "../GlobeIcon";
import s from "./Sidebar.module.css";

/** Build the savedPaneLayouts key for the currently active repo+branch */
function currentRepoLayoutKey(): string | undefined {
	const repoPath = repositoriesStore.state.activeRepoPath;
	if (!repoPath) return undefined;
	const repo = repositoriesStore.state.repositories[repoPath];
	if (!repo?.activeWorkspaceId) return undefined;
	return paneLayoutKey(repoPath, repo.activeWorkspaceId);
}

export const GlobalWorkspaceEntry: Component = () => {
	// One-way: clicking always means "show the manual Global Workspace." It
	// never deactivates — the only way to leave it is clicking a terminal
	// within a repo in the sidebar (see navigateToTerminal.ts).
	const handleClick = () => {
		if (globalWorkspaceStore.isManualWorkspaceActive()) return;
		globalWorkspaceStore.setScope(MANUAL_SCOPE);
		if (!globalWorkspaceStore.isActive()) {
			globalWorkspaceStore.activate(currentRepoLayoutKey());
		}
	};

	return (
		<Show when={globalWorkspaceStore.getLiveManualMembers().length > 0}>
			<div
				class={`${s.globalWorkspaceEntry} ${globalWorkspaceStore.isManualWorkspaceActive() ? s.globalWorkspaceActive : ""}`}
				onClick={handleClick}
			>
				<GlobeIcon />
				<span class={s.globalWorkspaceLabel}>Global Workspace</span>
				<span class={s.globalWorkspaceBadge}>{globalWorkspaceStore.getLiveManualMembers().length}</span>
			</div>
		</Show>
	);
};
