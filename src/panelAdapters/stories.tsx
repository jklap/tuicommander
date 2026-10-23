import { type Component, lazy, onMount, Show, Suspense } from "solid-js";
import { initPanelWindow } from "../hooks/initPanelWindow";
import { closePanel, type PanelAdapter } from "../panelRouter";
import { repositoriesStore } from "../stores/repositories";
import { storiesUi } from "../stores/storiesUi";

const StoriesDialog = lazy(() =>
	import("../components/StoriesDialog/StoriesDialog").then((module) => ({ default: module.StoriesDialog })),
);

const DetachedStories: Component<{ params: URLSearchParams }> = (props) => {
	const project = props.params.get("project");
	onMount(() => {
		void initPanelWindow();
	});
	return (
		<Show when={project} keyed>
			{(path) => (
				<Suspense>
					<StoriesDialog project={path} mode="detached" onClose={() => void closePanel("stories")} />
				</Suspense>
			)}
		</Show>
	);
};

export const storiesPanelAdapter: PanelAdapter = {
	id: "stories",
	title: "Plans and Stories",
	defaultSize: { width: 1120, height: 800 },
	detachParams: (): Record<string, string> => {
		// DEFERRED (2026-10-23) — shared detached-panel prefs retain labels, not params;
		// restart falls back to the active project, like other repo-scoped panels.
		const project = storiesUi.project() ?? repositoriesStore.state.activeRepoPath;
		return project ? { project } : {};
	},
	onDetach: () => storiesUi.close(),
	toggle: () => storiesUi.open(storiesUi.project() ?? repositoriesStore.state.activeRepoPath),
	Component: DetachedStories,
};
