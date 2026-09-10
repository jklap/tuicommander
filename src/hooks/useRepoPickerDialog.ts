import { createSignal } from "solid-js";
import { repositoriesStore } from "../stores/repositories";

/** What the user picked in the RepoPickerDialog for a Finder-invoked path
 *  that resolved to no repo (see `resolvePlacementForCwd`). `null` means the
 *  dialog was cancelled/closed with no choice made. */
export type RepoChoice = { kind: "repo"; repoPath: string } | { kind: "register" } | { kind: "unattached" };

export interface RepoPickerDialogState {
	path: string;
	repos: Array<{ path: string; displayName: string }>;
}

/**
 * Hook for the "which repo?" picker, mirroring `useConfirmDialog`'s
 * queue-of-one-promise-per-request shape: concurrent `chooseRepoForPath`
 * calls enqueue instead of overwriting a single resolver, so no request's
 * promise is silently orphaned.
 */
export function useRepoPickerDialog() {
	const [dialogState, setDialogState] = createSignal<RepoPickerDialogState | null>(null);
	const queue: Array<{ path: string; resolve: (value: RepoChoice | null) => void }> = [];

	function showHead() {
		const head = queue[0];
		if (!head) {
			setDialogState(null);
			return;
		}
		setDialogState({
			path: head.path,
			repos: repositoriesStore.getAllReposOrdered().map((repo) => ({ path: repo.path, displayName: repo.displayName })),
		});
	}

	function chooseRepoForPath(path: string): Promise<RepoChoice | null> {
		return new Promise((resolve) => {
			queue.push({ path, resolve });
			if (queue.length === 1) showHead();
		});
	}

	function settle(value: RepoChoice | null) {
		const head = queue.shift();
		head?.resolve(value);
		showHead();
	}

	return {
		dialogState,
		chooseRepoForPath,
		handleChooseRepo: (repoPath: string) => settle({ kind: "repo", repoPath }),
		handleRegister: () => settle({ kind: "register" }),
		handleUnattached: () => settle({ kind: "unattached" }),
		handleClose: () => settle(null),
	};
}
