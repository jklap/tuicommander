import type { WorkspaceState } from "../../stores/workspaceIdentity";

/**
 * The warm status the backend attaches to every worktree in `get_repo_structure`
 * / `get_worktree_paths` (`GET /worktrees/paths`): tuic-git `warm_status`, i.e.
 * `{status: "pending", phase?, copied?, total?}` while the post-create chain
 * runs, `{status: "done"}` / `{status: "failed", reason}` after.
 */
export interface WarmArtifactsStatus {
	status: string;
	phase?: string;
	copied?: number;
	total?: number;
	reason?: string;
}

type WarmState = WorkspaceState["warmState"];

/** Worktree paths a `worktree-warm-*` event has spoken for during this page
 *  session. Once one has, the events own that row's badge: the list is a
 *  cached (up to `GIT_CACHE_TTL`) snapshot and would only move it backwards. */
const eventDriven = new Set<string>();

/** Same directory regardless of separator flavour or a trailing slash, matching
 *  `sameDir` (utils/repoOwnership.ts) so an event path and a list path agree. */
function dirKey(path: string): string {
	return path
		.split(/[\\/]+/)
		.filter(Boolean)
		.join("/");
}

/** Called by every `worktree-warm-*` handler (useAppInit.ts `updateWarmState`). */
export function noteWarmEvent(worktreePath: string): void {
	eventDriven.add(dirKey(worktreePath));
}

/**
 * The badge state a structure refresh should write for one worktree, or
 * `undefined` to leave the row alone.
 *
 * This is what shows "Warming x/y" after a reload mid-warm: the events that
 * drive the badge are not replayed, so without it the row stayed blank (or kept
 * a stale persisted badge) until the next event. Until an event arrives for
 * the path, the backend's own warm status decides — a still-pending warm copy
 * seeds the badge, anything else (done, failed, the later file-sync/Setup
 * Script phase, no status at all) clears it.
 */
export function warmStateFromList(worktreePath: string, warm: WarmArtifactsStatus | undefined): WarmState | undefined {
	if (eventDriven.has(dirKey(worktreePath))) return undefined;
	if (warm?.status === "pending" && warm.phase === "warming" && typeof warm.total === "number") {
		return { status: "warming", copied: warm.copied ?? 0, total: warm.total };
	}
	return null;
}

/** Test-only: forget every event-driven path. */
export function _resetWarmEventsForTest(): void {
	eventDriven.clear();
}
