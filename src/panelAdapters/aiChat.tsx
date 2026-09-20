import { type Component, onMount } from "solid-js";
import { AIChatPanel } from "../components/AIChatPanel/AIChatPanel";
import { initPanelWindow } from "../hooks/initPanelWindow";
import type { PanelAdapter } from "../panelRouter";
import { repositoriesStore } from "../stores/repositories";
import { uiStore } from "../stores/ui";

/**
 * The AI Chat panel in its own window.
 *
 * The repository comes in as a parameter rather than off `repositoriesStore`:
 * a panel window is a separate WebView where that store is never hydrated —
 * App returns at `renderPanelMode()` before any main-window effect — so a
 * detached panel that read the store would find no repository and could never
 * open a connection.
 */
const DetachedAIChatPanel: Component<{ params: URLSearchParams }> = (props) => {
	const repoPath = props.params.get("repoPath");
	const fsRoot = props.params.get("fsRoot");

	onMount(() => {
		void initPanelWindow();
	});

	return <AIChatPanel visible={true} repoPath={repoPath} fsRoot={fsRoot} onClose={() => window.close()} />;
};

/** The worktree the active repository is currently on, where it is on one. */
function activeFsRoot(): string | undefined {
	const active = repositoriesStore.getActive();
	if (!active?.activeWorkspaceId) return undefined;
	return active.workspaces[active.activeWorkspaceId]?.worktreePath || active.path;
}

export const aiChatPanelAdapter: PanelAdapter = {
	id: "ai-chat",
	title: "AI Chat",
	defaultSize: { width: 420, height: 700 },
	toggle: () => uiStore.toggleAiChatPanel(),
	onDetach: () => uiStore.setAiChatPanelVisible(false),
	detachParams: () => {
		const repoPath = repositoriesStore.state.activeRepoPath;
		const fsRoot = activeFsRoot();
		return {
			...(repoPath ? { repoPath } : {}),
			...(fsRoot ? { fsRoot } : {}),
		};
	},
	Component: DetachedAIChatPanel,
};
