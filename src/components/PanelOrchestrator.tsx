import { type Component, Show } from "solid-js";
import { diffTabsStore } from "../stores/diffTabs";
import { globalWorkspaceStore, MANUAL_SCOPE } from "../stores/globalWorkspace";
import { settingsStore } from "../stores/settings";
import { uiStore } from "../stores/ui";
import { sendTextToActiveTerminal } from "../utils/sendToActiveTerminal";
import { AIChatPanel } from "./AIChatPanel";
import { AiTriagePanel } from "./AiTriagePanel";
import { FileBrowserPanel } from "./FileBrowserPanel";
import { GitPanel } from "./GitPanel/GitPanel";
import { MarkdownPanel } from "./MarkdownPanel";
import { NotesPanel } from "./NotesPanel";
import { OutlinePanel } from "./OutlinePanel";
import { ReferencesPanel } from "./ReferencesPanel";

export interface PanelOrchestratorProps {
	repoPath: string | null;
	/** Effective filesystem root (worktree path when on a linked worktree) */
	fsRoot?: string | null;
	onFileOpen: (repoPath: string, filePath: string, line?: number) => void;
}

/**
 * Whether the hand-promoted, cross-repo global workspace is showing.
 *
 * A per-repo auto-consolidated workspace (#e767) has a single, well-defined
 * repo — `props.repoPath`/`fsRoot` still resolve correctly — so it must not
 * suppress these panels the way the manual, potentially cross-repo workspace
 * does.
 */
function manualWorkspaceActive(): boolean {
	return globalWorkspaceStore.isActive() && globalWorkspaceStore.getScope() === MANUAL_SCOPE;
}

export const PanelOrchestrator: Component<PanelOrchestratorProps> = (props) => {
	return (
		<>
			<Show when={!uiStore.isDetached("file-browser")}>
				<FileBrowserPanel
					visible={uiStore.state.fileBrowserPanelVisible && !manualWorkspaceActive()}
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
				<NotesPanel
					visible={uiStore.state.notesPanelVisible}
					repoPath={props.repoPath}
					onClose={() => uiStore.toggleNotesPanel()}
					onSendToTerminal={(text) => void sendTextToActiveTerminal(text)}
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
					visible={uiStore.state.gitPanelVisible && !manualWorkspaceActive()}
					repoPath={props.repoPath}
					fsRoot={props.fsRoot}
					onClose={() => uiStore.toggleGitPanel()}
					requestedTab={uiStore.state.gitPanelRequestedTab}
					onOpenDiff={diffTabsStore.add.bind(diffTabsStore)}
				/>
			</Show>

			<Show when={settingsStore.isAiChatEnabled() && !uiStore.isDetached("ai-chat")}>
				<AIChatPanel visible={uiStore.state.aiChatPanelVisible} onClose={() => uiStore.toggleAiChatPanel()} />
			</Show>

			<Show when={uiStore.state.aiTriagePanelVisible}>
				<AiTriagePanel visible={true} repoPath={props.repoPath} onClose={() => uiStore.toggleAiTriagePanel()} />
			</Show>
		</>
	);
};
