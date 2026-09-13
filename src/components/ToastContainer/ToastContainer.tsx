import type { Component } from "solid-js";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";
import { type Toast, toastsStore } from "../../stores/toasts";
import { navigateToTerminal } from "../../utils/navigateToTerminal";
import { pathBasename } from "../../utils/pathUtils";
import { ToastList } from "./ToastList";

/**
 * Dismiss, and take the user to the terminal that raised the toast when there
 * is one to go to. An agent's `ui action=toast` says something happened in one
 * of ~25 open tabs; without this the user is told the news and then left to
 * find the speaker. A toast with no session, or one whose tab has since closed,
 * still just dismisses — the lookup happens here rather than in setActive so a
 * closed tab is a silent no-op instead of a warning.
 *
 * `navigateToTerminal`, not `terminalsStore.setActive`: the speaker is usually in
 * ANOTHER repo, and setActive moves only the active terminal. The sidebar and the
 * tab strip filter on `activeRepoPath`, so on its own it left the pane drawing a
 * terminal from a repo the user was not looking at, with no tab for it in the
 * strip — the same three-state split a cd used to cause.
 */
function dismissAndReveal(toast: Toast): void {
	toastsStore.remove(toast.id);
	if (!toast.sessionId) return;
	const terminalId = terminalsStore.findBySessionId(toast.sessionId);
	if (terminalId) navigateToTerminal(terminalId);
}

/** The repo a toast came from, for the badge. Prefers what the backend resolved
 *  from the caller's cwd; falls back to the repo owning the speaking terminal, so
 *  a toast still names its repo when the cwd matched none. */
function toastRepoName(toast: Toast): string | null {
	const fromOrigin = toast.repoPath;
	if (fromOrigin) return pathBasename(fromOrigin);
	if (!toast.sessionId) return null;
	const terminalId = terminalsStore.findBySessionId(toast.sessionId);
	const repoPath = terminalId ? repositoriesStore.getRepoPathForTerminal(terminalId) : null;
	return repoPath ? pathBasename(repoPath) : null;
}

export const ToastContainer: Component = () => {
	return <ToastList onDismiss={dismissAndReveal} repoName={toastRepoName} />;
};
