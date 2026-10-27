import { type Component, createEffect, createSignal, lazy, Show, Suspense, untrack } from "solid-js";
import { diffTabsStore } from "../stores/diffTabs";
import { globalWorkspaceStore } from "../stores/globalWorkspace";
import { progressStore } from "../stores/progress";
import { settingsStore } from "../stores/settings";
import { storiesUi } from "../stores/storiesUi";
import { uiStore } from "../stores/ui";
import {
	canQueueToActiveTerminal,
	queueTextToActiveTerminal,
	sendTextToActiveTerminal,
} from "../utils/sendToActiveTerminal";
import { AutomationsHost } from "./AutomationsDialog/AutomationsHost";
import { OutlinePanel } from "./OutlinePanel";
import { ProgressDialog } from "./ProgressDialog";
import { ReferencesPanel } from "./ReferencesPanel";

// The four side panels are code-split out of the entry chunk. Each is mounted on
// first open (see `createMountedOnFirstShow`) and then stays mounted, so its
// state survives hide/show exactly as it did when it was always mounted.
const FileBrowserPanel = lazy(() =>
	import("./FileBrowserPanel").then((module) => ({ default: module.FileBrowserPanel })),
);
const GitPanel = lazy(() => import("./GitPanel/GitPanel").then((module) => ({ default: module.GitPanel })));
const IdeasPanel = lazy(() => import("./IdeasPanel").then((module) => ({ default: module.IdeasPanel })));
const MarkdownPanel = lazy(() => import("./MarkdownPanel").then((module) => ({ default: module.MarkdownPanel })));

const StoriesDialog = lazy(() =>
	import("./StoriesDialog/StoriesDialog").then((module) => ({ default: module.StoriesDialog })),
);
const AIChatPanel = lazy(() => import("./AIChatPanel").then((module) => ({ default: module.AIChatPanel })));

/** True from the first time `visible()` is true onward (initial value counts). */
function createMountedOnFirstShow(visible: () => boolean): () => boolean {
	const [mounted, setMounted] = createSignal(untrack(visible));
	createEffect(() => {
		if (visible()) setMounted(true);
	});
	return mounted;
}

export interface PanelOrchestratorProps {
	repoPath: string | null;
	/** Effective filesystem root (worktree path when on a linked worktree) */
	fsRoot?: string | null;
	onOpenSettings?: (tab: string, section?: string) => void;
	onFileOpen: (repoPath: string, filePath: string, line?: number) => void;
}

export const PanelOrchestrator: Component<PanelOrchestratorProps> = (props) => {
	const [aiChatRequested, setAiChatRequested] = createSignal(uiStore.state.aiChatPanelVisible);
	createEffect(() => {
		if (uiStore.state.aiChatPanelVisible) setAiChatRequested(true);
	});
	const fileBrowserVisible = () =>
		uiStore.state.fileBrowserPanelVisible && !globalWorkspaceStore.isManualWorkspaceActive();
	const gitVisible = () => uiStore.state.gitPanelVisible && !globalWorkspaceStore.isManualWorkspaceActive();
	const fileBrowserMounted = createMountedOnFirstShow(fileBrowserVisible);
	const markdownMounted = createMountedOnFirstShow(() => uiStore.state.markdownPanelVisible);
	const ideasMounted = createMountedOnFirstShow(() => uiStore.state.ideasPanelVisible);
	const gitMounted = createMountedOnFirstShow(gitVisible);
	return (
		<>
			<AutomationsHost />
			<Show when={!uiStore.isDetached("file-browser") && fileBrowserMounted()}>
				<Suspense>
					<FileBrowserPanel
						visible={fileBrowserVisible()}
						repoPath={props.repoPath}
						fsRoot={props.fsRoot}
						onClose={() => uiStore.toggleFileBrowserPanel()}
						onFileOpen={props.onFileOpen}
					/>
				</Suspense>
			</Show>

			<Show when={!uiStore.isDetached("markdown") && markdownMounted()}>
				<Suspense>
					<MarkdownPanel
						visible={uiStore.state.markdownPanelVisible}
						repoPath={props.repoPath}
						fsRoot={props.fsRoot}
						onClose={() => uiStore.toggleMarkdownPanel()}
					/>
				</Suspense>
			</Show>

			<Show when={!uiStore.isDetached("notes") && ideasMounted()}>
				<Suspense>
					<IdeasPanel
						visible={uiStore.state.ideasPanelVisible}
						repoPath={props.repoPath}
						onClose={() => uiStore.toggleIdeasPanel()}
						onSendToTerminal={(text) => void sendTextToActiveTerminal(text)}
						onQueueToTerminal={canQueueToActiveTerminal() ? (text) => void queueTextToActiveTerminal(text) : undefined}
					/>
				</Suspense>
			</Show>

			<Show when={!uiStore.isDetached("outline") && uiStore.state.outlinePanelVisible}>
				<OutlinePanel visible={true} onClose={() => uiStore.toggleOutlinePanel()} />
			</Show>

			<Show when={!uiStore.isDetached("references") && uiStore.state.referencesPanelVisible}>
				<ReferencesPanel visible={true} onClose={() => uiStore.toggleReferencesPanel()} />
			</Show>

			<Show when={!uiStore.isDetached("git") && gitMounted()}>
				<Suspense>
					<GitPanel
						visible={gitVisible()}
						repoPath={props.repoPath}
						fsRoot={props.fsRoot}
						onClose={() => uiStore.toggleGitPanel()}
						requestedTab={uiStore.state.gitPanelRequestedTab}
						onOpenDiff={diffTabsStore.add.bind(diffTabsStore)}
					/>
				</Suspense>
			</Show>

			<Show when={settingsStore.isAiChatEnabled() && !uiStore.isDetached("ai-chat") && aiChatRequested()}>
				<Suspense>
					<AIChatPanel
						visible={uiStore.state.aiChatPanelVisible}
						repoPath={props.repoPath}
						fsRoot={props.fsRoot}
						onClose={() => uiStore.toggleAiChatPanel()}
						onOpenSettings={props.onOpenSettings}
					/>
				</Suspense>
			</Show>

			<Show when={progressStore.dialogVisible()}>
				<ProgressDialog />
			</Show>
			<Show when={storiesUi.visible()}>
				<Show when={storiesUi.project()} keyed>
					{(project) => (
						<Suspense>
							<StoriesDialog project={project} onClose={() => storiesUi.close()} />
						</Suspense>
					)}
				</Show>
			</Show>
		</>
	);
};
