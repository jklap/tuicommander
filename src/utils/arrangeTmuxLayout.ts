import { appLogger } from "../stores/appLogger";
import { globalWorkspaceStore } from "../stores/globalWorkspace";
import {
	arrangeLayoutState,
	type PaneLayoutState,
	paneLayoutStore,
	pruneLayoutToLiveTerminals,
} from "../stores/paneLayout";
import { repositoriesStore } from "../stores/repositories";
import { paneLayoutKey, savedPaneLayouts } from "../stores/savedPaneLayouts";
import { terminalsStore } from "../stores/terminals";
import { filterValidTerminals } from "./terminalFilter";

interface Owner {
	repoPath: string;
	workspaceId: string;
}

/** The repo+branch whose `terminals` list holds `termId`, or null when none claims it yet. */
function ownerOf(termId: string): Owner | null {
	const repoPath = repositoriesStore.getRepoPathForTerminal(termId);
	const repo = repoPath ? repositoriesStore.state.repositories[repoPath] : undefined;
	if (!repoPath || !repo) return null;
	for (const [workspaceId, workspace] of Object.entries(repo.workspaces)) {
		if (workspace.terminals.includes(termId)) return { repoPath, workspaceId };
	}
	return null;
}

const EMPTY_LAYOUT: PaneLayoutState = { root: null, groups: {}, activeGroupId: null };

/**
 * Arrange tmux-shim terminals (`select-layout tiled`/`main-vertical`) into a
 * split in the layout of the branch that OWNS them.
 *
 * `paneLayoutStore` only ever holds the branch on screen. Applying a swarm's
 * arrangement to it unconditionally put the split into whichever branch the
 * user happened to be looking at (or silently dropped it when that branch had
 * its own split), and nothing recorded it for the swarm's own branch. A
 * branch that isn't on screen is arranged in its saved layout instead, which
 * `resolvePaneLayoutForBranch` restores when the user gets there.
 *
 * Terminals no branch claims yet, and everything while the manual Global
 * Workspace is showing (its layout is the live one), keep the live behavior.
 */
export function arrangeSwarmLayout(termIds: string[], layout: string): void {
	if (globalWorkspaceStore.isManualWorkspaceActive()) {
		paneLayoutStore.arrangeSessionsAsLayout(termIds, layout);
		return;
	}

	const activeRepoPath = repositoriesStore.state.activeRepoPath;
	const activeWorkspaceId = activeRepoPath
		? repositoriesStore.state.repositories[activeRepoPath]?.activeWorkspaceId
		: null;

	const live: string[] = [];
	const offscreen = new Map<string, { owner: Owner; ids: string[] }>();
	for (const id of termIds) {
		const owner = ownerOf(id);
		if (!owner || (owner.repoPath === activeRepoPath && owner.workspaceId === activeWorkspaceId)) {
			live.push(id);
			continue;
		}
		const key = paneLayoutKey(owner.repoPath, owner.workspaceId);
		const entry = offscreen.get(key) ?? { owner, ids: [] };
		entry.ids.push(id);
		offscreen.set(key, entry);
	}

	if (live.length > 0) paneLayoutStore.arrangeSessionsAsLayout(live, layout);
	for (const [key, { owner, ids }] of offscreen) arrangeSavedLayout(key, owner, ids, layout);
}

function arrangeSavedLayout(key: string, owner: Owner, ids: string[], layout: string): void {
	const workspace = repositoriesStore.state.repositories[owner.repoPath]?.workspaces[owner.workspaceId];
	const valid = new Set(
		filterValidTerminals(workspace?.terminals, terminalsStore.getIds()).filter((id) => !terminalsStore.isDetached(id)),
	);
	// Prune first: a member closed since the last arrangement would otherwise read as an
	// unrelated pane and make the whole request bail.
	const saved = savedPaneLayouts.get(key);
	const base = (saved && pruneLayoutToLiveTerminals(saved, valid)) || EMPTY_LAYOUT;

	const result = arrangeLayoutState(base, ids, layout);
	if (!result.ok) {
		if (result.reason === "unrelated-panes") {
			appLogger.warn(
				"app",
				`tmux select-layout request skipped for ${owner.repoPath} (${owner.workspaceId}): its saved split has ` +
					"other panes this swarm doesn't own — leaving it as-is rather than replacing it.",
			);
		}
		return;
	}
	// A saved layout only ever holds a real split (see `savePaneLayoutForBranch`).
	if (result.state.root?.type === "branch") savedPaneLayouts.set(key, result.state);
}
