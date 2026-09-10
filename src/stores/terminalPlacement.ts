/**
 * Where a new terminal tab should be filed, given only its cwd.
 *
 * This is the placement ladder used both when the backend hands us a session
 * whose cwd wasn't chosen by the user (reconnect, MCP-spawned agent) and when
 * an external trigger (e.g. a Finder "open here" invocation) hands us a raw
 * filesystem path with no other context:
 *
 *   1. a registered repo or linked worktree owns the cwd → that repo/branch;
 *   2. otherwise, the currently active repo (if it has an active branch);
 *   3. otherwise, nothing — the caller must decide what "no answer" means.
 *
 * `isGuess: true` on the step-2 result means exactly what it says: nothing
 * actually claims this cwd, and the terminal is being parked in the active
 * repo only because it needs somewhere to render. Callers that record
 * ownership (`terminalsStore.setRepoPath`) must NOT use this guessed
 * `repoPath` as the recorded owner — record `null` instead so
 * `reconcileTerminalOwnership` can walk the tab home once its real repo is
 * registered. See `assignSessionToRepoBranch` (`hooks/useAppInit.ts`) for the
 * call site that keeps this distinction.
 */

import type { RepoOwner } from "../utils/repoOwnership";
import { placementBranchFor, repositoriesStore, resolveRepoOwner } from "./repositories";

export interface TerminalPlacement {
	repoPath: string;
	branchName: string;
	/** True when no registered repo actually owns the cwd — this is a parking
	 *  spot (the active repo), not a real placement. */
	isGuess: boolean;
}

/**
 * Core of the ladder, taking an already-resolved owner. Split out so a caller
 * that already called `resolveRepoOwner` for its own purposes (e.g.
 * `assignSessionToRepoBranch` in `hooks/useAppInit.ts`, which also needs the
 * raw owner to decide what to record via `setRepoPath`) doesn't pay for the
 * same `resolveRepoOwnerIn` scan twice.
 */
export function resolvePlacementForOwner(owner: RepoOwner | null): TerminalPlacement | null {
	if (owner) {
		const branchName = placementBranchFor(owner);
		if (branchName) {
			return { repoPath: owner.repoPath, branchName, isGuess: false };
		}
	}

	const fallbackRepo = repositoriesStore.state.activeRepoPath;
	const fallbackBranch = fallbackRepo ? repositoriesStore.get(fallbackRepo)?.activeBranch : null;
	if (fallbackRepo && fallbackBranch) {
		return { repoPath: fallbackRepo, branchName: fallbackBranch, isGuess: true };
	}

	return null;
}

export function resolvePlacementForCwd(cwd: string | null | undefined): TerminalPlacement | null {
	return resolvePlacementForOwner(resolveRepoOwner(cwd));
}
