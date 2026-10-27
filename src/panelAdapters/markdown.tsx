import { type Component, lazy, onMount, Suspense } from "solid-js";
import { initPanelWindow } from "../hooks/initPanelWindow";
import type { PanelAdapter } from "../panelRouter";
import { repositoriesStore } from "../stores/repositories";
import { uiStore } from "../stores/ui";

const MarkdownPanel = lazy(() =>
	import("../components/MarkdownPanel").then((module) => ({ default: module.MarkdownPanel })),
);

const DetachedMarkdownPanel: Component<{ params: URLSearchParams }> = (props) => {
	const repoPath = props.params.get("repoPath");
	const fsRoot = props.params.get("fsRoot");

	onMount(() => {
		void initPanelWindow();
	});

	return (
		<Suspense>
			<MarkdownPanel
				visible={true}
				repoPath={repoPath}
				fsRoot={fsRoot}
				mode="detached"
				onClose={() => window.close()}
			/>
		</Suspense>
	);
};

function getActiveFsRoot(): string | undefined {
	const activeRepo = repositoriesStore.getActive();
	if (!activeRepo?.activeWorkspaceId) return undefined;
	return activeRepo.workspaces[activeRepo.activeWorkspaceId]?.worktreePath || activeRepo.path;
}

export const markdownPanelAdapter: PanelAdapter = {
	id: "markdown",
	title: "Markdown",
	defaultSize: { width: 500, height: 700 },
	toggle: () => uiStore.toggleMarkdownPanel(),
	onDetach: () => uiStore.setMarkdownPanelVisible(false),
	detachParams: () => {
		const repoPath = repositoriesStore.state.activeRepoPath;
		const fsRoot = getActiveFsRoot();
		return {
			...(repoPath ? { repoPath } : {}),
			...(fsRoot ? { fsRoot } : {}),
		};
	},
	Component: DetachedMarkdownPanel,
};
