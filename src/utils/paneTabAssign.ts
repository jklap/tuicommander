import { type PaneTabType, paneLayoutStore } from "../stores/paneLayout";
import { setOnTabAdded } from "../stores/tabManager";
import { terminalsStore } from "../stores/terminals";

export interface AssignTabOptions {
	/** The tab is agent-spawned: it may dock into an agent-owned split, and the viewed-orphan check is skipped. */
	agent?: boolean;
	/** The tab the user was looking at before this one was created; `null` = don't check.
	 *  Defaults to the active terminal. If it is an orphan overlaying the split (not in any
	 *  pane), the new tab stays an orphan too — it must not vanish into a split that is
	 *  not on screen. */
	displayedId?: string | null;
}

/** Assign a tab to the active pane group when split mode is on.
 *  No-op when not in split mode. Safe to call unconditionally after tab creation.
 *  `activate=false` docks the tab without switching the pane to it.
 *  Skipped (the tab stays a full-page orphan) when the split is agent-owned (tmux shim) and the
 *  caller isn't an agent, or when the user is viewing an orphan tab over the split. */
export function assignTabToActiveGroup(
	tabId: string,
	type: PaneTabType,
	activate = true,
	opts: AssignTabOptions = {},
): void {
	if (!paneLayoutStore.isSplit()) return;
	if (paneLayoutStore.isAgentOwned() && !opts.agent) return;
	const displayedId = opts.displayedId === undefined ? terminalsStore.state.activeId : opts.displayedId;
	if (!opts.agent && displayedId && displayedId !== tabId && !paneLayoutStore.getGroupForTab(displayedId)) return;
	const activeGroupId = paneLayoutStore.state.activeGroupId;
	if (!activeGroupId) return;
	paneLayoutStore.addTab(activeGroupId, { id: tabId, type }, activate);
}

/** Register the global hook so terminal tabs auto-assign to active pane group.
 *  Non-terminal tabs (panels, diff, editor) start unassigned — drag into a
 *  split pane to dock them. Call once during app initialization. */
export function initPaneTabAssignment(): void {
	setOnTabAdded(() => {
		// Only terminals are auto-assigned (handled by pty spawn logic).
		// Non-terminal tabs start as orphans so they render full-page.
	});
}
