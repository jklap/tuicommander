import type { Component } from "solid-js";
import { appLogger } from "../../stores/appLogger";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";
import { type Toast, toastsStore } from "../../stores/toasts";
import { uiStore } from "../../stores/ui";
import { navigateToTerminal } from "../../utils/navigateToTerminal";
import { pathBasename } from "../../utils/pathUtils";
import { type RepoAction, ToastList } from "./ToastList";

/** Closing the toast never navigates; the explicit repo action owns navigation. */
function dismiss(toast: Toast): void {
	toastsStore.remove(toast.id);
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

const repoActions = new WeakMap<Toast, RepoAction>();

function toastRepoAction(toast: Toast): RepoAction | null {
	const repoPath = toast.repoPath;
	if (!repoPath || repoPath === repositoriesStore.state.activeRepoPath || !repositoriesStore.get(repoPath)) {
		return null;
	}

	let action = repoActions.get(toast);
	if (!action) {
		action = {
			label: "Go to repo",
			onClick: () => {
				// The repository can disappear while the toast is visible. Re-check at
				// activation time so the button never selects a stale path. Returning
				// false keeps the toast visible so the failed navigation is not silent.
				if (!repositoriesStore.get(repoPath)) {
					appLogger.warn("app", "Go to repo: the repository is no longer registered", { repoPath });
					return false;
				}
				if (toast.sessionId) {
					const terminalId = terminalsStore.findBySessionId(toast.sessionId);
					if (terminalId) {
						navigateToTerminal(terminalId);
						return true;
					}
					appLogger.warn("app", "Go to repo: the originating terminal is no longer open", {
						repoPath,
						sessionId: toast.sessionId,
					});
				}
				repositoriesStore.setActive(repoPath);
				return true;
			},
		};
		repoActions.set(toast, action);
	}

	return action;
}

export const ToastContainer: Component = () => {
	return (
		<ToastList
			onDismiss={dismiss}
			repoName={toastRepoName}
			repoAction={toastRepoAction}
			rightInset={uiStore.state.aiChatPanelMeasuredWidth}
		/>
	);
};
