import { type Component, lazy, onMount, Suspense } from "solid-js";
import { initPanelWindow } from "../hooks/initPanelWindow";
import type { PanelAdapter } from "../panelRouter";
import { repositoriesStore } from "../stores/repositories";
import { uiStore } from "../stores/ui";
import { openTerminalFilePath } from "../utils/filePreview";

const AIChatPanel = lazy(() =>
	import("../components/AIChatPanel/AIChatPanel").then((module) => ({ default: module.AIChatPanel })),
);

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

	return (
		<Suspense>
			<AIChatPanel visible={true} repoPath={repoPath} fsRoot={fsRoot} onClose={() => window.close()} />
		</Suspense>
	);
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
	handleAction: (action, data) => {
		if ((action !== "open-file" && action !== "open-directory") || !data || typeof data !== "object") return;
		const { path, line, col } = data as { path?: unknown; line?: unknown; col?: unknown };
		if (typeof path !== "string" || !path) return;
		if (action === "open-directory") {
			uiStore.setFileBrowserExternalRoot(path);
			uiStore.setFileBrowserPanelVisible(true);
			return;
		}
		if (line !== undefined && (!Number.isInteger(line) || Number(line) < 1)) return;
		if (col !== undefined && (!Number.isInteger(col) || Number(col) < 1)) return;
		if (line !== undefined) openTerminalFilePath(path, undefined, line as number, col as number | undefined);
		else openTerminalFilePath(path);
	},
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
