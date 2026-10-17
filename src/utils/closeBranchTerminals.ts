import { invoke } from "../invoke";
import { appLogger } from "../stores/appLogger";
import { repositoriesStore } from "../stores/repositories";
import { terminalsStore } from "../stores/terminals";

/**
 * End the PTY of every terminal filed under one workspace (post-merge cleanup).
 *
 * `branch.terminals` holds terminal-store ids ("term-3"); `close_pty` wants the PTY session id. The
 * three copies of this that lived in `GitHubPanel`, `RemoteOnlyPrPopover` and `PrDetailPopover`
 * sent the terminal id, so every close failed ("Session not found") and was only logged. A tab
 * with no PTY yet (a restore placeholder the user never opened) has nothing to close.
 */
export async function closeBranchTerminalPtys(repoPath: string, workspaceId: string): Promise<void> {
	const branch = repositoriesStore.get(repoPath)?.workspaces[workspaceId];
	if (!branch) return;
	for (const termId of branch.terminals) {
		const sessionId = terminalsStore.get(termId)?.sessionId;
		if (!sessionId) continue;
		try {
			await invoke("close_pty", { sessionId, cleanupWorktree: false });
		} catch (err) {
			appLogger.warn("git", `close_pty failed for terminal ${termId}`, err);
		}
	}
}
