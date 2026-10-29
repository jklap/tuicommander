import { type PaneLayoutState, paneLayoutStore, pruneLayoutToLiveTerminals } from "../stores/paneLayout";
import { paneLayoutKey, savedPaneLayouts } from "../stores/savedPaneLayouts";

/** Saves the OUTGOING repo+branch's pane layout (if split) so it can be restored later,
 *  mirroring `handleBranchSelectInner`'s own save-on-leave step. Shared so any path that
 *  flips the active repo/branch — not just a full branch select — leaves the layout it's
 *  abandoning in a state the next branch select can find. */
export function savePaneLayoutForBranch(repoPath: string, workspaceId: string): void {
	if (paneLayoutStore.isSplit()) {
		savedPaneLayouts.set(paneLayoutKey(repoPath, workspaceId), paneLayoutStore.serialize());
	} else {
		// Clear any stale layout if the user unsplit while on this branch.
		savedPaneLayouts.delete(paneLayoutKey(repoPath, workspaceId));
	}
}

/** Resolves `paneLayoutStore` to whatever repo+branch's own saved/disk layout implies —
 *  restoring it, minus any terminal that no longer exists (a closed terminal costs the
 *  split its own pane, not the whole split), otherwise resetting to a flat single pane.
 *  Any path that flips the active repo/branch must call this before touching pane tabs,
 *  or the OUTGOING branch's split tree stays live under the INCOMING branch's terminals. */
export function resolvePaneLayoutForBranch(repoPath: string, workspaceId: string, validTerminals: string[]): void {
	const layoutKey = paneLayoutKey(repoPath, workspaceId);
	const valid = new Set(validTerminals);
	const savedLayout = savedPaneLayouts.get(layoutKey);
	if (savedLayout) {
		const pruned = pruneLayoutToLiveTerminals(savedLayout, valid);
		if (pruned) {
			// Keep the cache in step so the closed terminal isn't re-pruned (or re-resurrected) later.
			savedPaneLayouts.set(layoutKey, pruned);
			paneLayoutStore.restore(pruned);
		} else {
			savedPaneLayouts.delete(layoutKey);
			paneLayoutStore.reset();
		}
	} else if (paneLayoutStore.consumeRestoredFromDisk()) {
		// Layout was loaded from disk at startup — keep what's still valid.
		const current: PaneLayoutState = paneLayoutStore.serialize();
		const pruned = pruneLayoutToLiveTerminals(current, valid);
		if (!pruned) paneLayoutStore.reset();
		else if (JSON.stringify(pruned) !== JSON.stringify(current)) paneLayoutStore.restore(pruned);
	} else {
		paneLayoutStore.reset();
	}
}
