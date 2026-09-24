import { type Component, lazy, Show } from "solid-js";
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
import { AIChatPanel } from "./AIChatPanel";
import { FileBrowserPanel } from "./FileBrowserPanel";
import { GitPanel } from "./GitPanel/GitPanel";
import { IdeasPanel } from "./IdeasPanel";
import { MarkdownPanel } from "./MarkdownPanel";
import { OutlinePanel } from "./OutlinePanel";
import { ProgressDialog } from "./ProgressDialog";
import { ReferencesPanel } from "./ReferencesPanel";

const StoriesDialog = lazy(() => import("./StoriesDialog/StoriesDialog").then((module) => ({ default: module.StoriesDialog })));

export interface PanelOrchestratorProps {
	repoPath: string | null;
	/** Effective filesystem root (worktree path when on a linked worktree) */
	fsRoot?: string | null;
	onFileOpen: (repoPath: string, filePath: string, line?: number) => void;
}

export const PanelOrchestrator: Component<PanelOrchestratorProps> = (props) => {
	return (
		<>
			<Show when={!uiStore.isDetached("file-browser")}>
				<FileBrowserPanel
					visible={uiStore.state.fileBrowserPanelVisible && !globalWorkspaceStore.isActive()}
					repoPath={props.repoPath}
					fsRoot={props.fsRoot}
					onClose={() => uiStore.toggleFileBrowserPanel()}
					onFileOpen={props.onFileOpen}
				/>
			</Show>

			<Show when={!uiStore.isDetached("markdown")}>
				<MarkdownPanel
					visible={uiStore.state.markdownPanelVisible}
					repoPath={props.repoPath}
					fsRoot={props.fsRoot}
					onClose={() => uiStore.toggleMarkdownPanel()}
				/>
			</Show>

			<Show when={!uiStore.isDetached("notes")}>
				<IdeasPanel
					visible={uiStore.state.ideasPanelVisible}
					repoPath={props.repoPath}
					onClose={() => uiStore.toggleIdeasPanel()}
					onSendToTerminal={(text) => void sendTextToActiveTerminal(text)}
					onQueueToTerminal={canQueueToActiveTerminal() ? (text) => void queueTextToActiveTerminal(text) : undefined}
				/>
			</Show>

			<Show when={!uiStore.isDetached("outline") && uiStore.state.outlinePanelVisible}>
				<OutlinePanel visible={true} onClose={() => uiStore.toggleOutlinePanel()} />
			</Show>

			<Show when={!uiStore.isDetached("references") && uiStore.state.referencesPanelVisible}>
				<ReferencesPanel visible={true} onClose={() => uiStore.toggleReferencesPanel()} />
			</Show>

			<Show when={!uiStore.isDetached("git")}>
				<GitPanel
					visible={uiStore.state.gitPanelVisible && !globalWorkspaceStore.isActive()}
					repoPath={props.repoPath}
					fsRoot={props.fsRoot}
					onClose={() => uiStore.toggleGitPanel()}
					requestedTab={uiStore.state.gitPanelRequestedTab}
					onOpenDiff={diffTabsStore.add.bind(diffTabsStore)}
				/>
			</Show>

			<Show when={settingsStore.isAiChatEnabled() && !uiStore.isDetached("ai-chat")}>
				<AIChatPanel
					visible={uiStore.state.aiChatPanelVisible}
					repoPath={props.repoPath}
					fsRoot={props.fsRoot}
					onClose={() => uiStore.toggleAiChatPanel()}
				/>
			</Show>

			<Show when={progressStore.dialogVisible()}>
				<ProgressDialog />
			</Show>
			<Show when={storiesUi.visible()}>
				<Show when={storiesUi.project()} keyed>
					{(project) => <StoriesDialog project={project} onClose={() => storiesUi.close()} />}
				</Show>
			</Show>
		</>
	);
};
