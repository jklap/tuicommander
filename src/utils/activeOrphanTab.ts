import { diffTabsStore } from "../stores/diffTabs";
import { editorTabsStore } from "../stores/editorTabs";
import { mdTabsStore } from "../stores/mdTabs";
import { paneLayoutStore } from "../stores/paneLayout";
import { terminalsStore } from "../stores/terminals";

/**
 * The active tab that is overlaying a split, if any: in split mode, an active
 * terminal/diff/markdown/editor tab that belongs to no pane group renders
 * full-page on top of the (hidden) split — see `TerminalArea.tsx`. Returns its
 * id, or null when nothing overlays the split (or there is no split).
 */
export function getActiveOrphanTabId(): string | null {
	if (!paneLayoutStore.isSplit()) return null;
	const ids = [
		terminalsStore.state.activeId,
		diffTabsStore.state.activeId,
		mdTabsStore.state.activeId,
		editorTabsStore.state.activeId,
	];
	return ids.find((id): id is string => !!id && !paneLayoutStore.getGroupForTab(id)) ?? null;
}

export type CloseTarget = { kind: "tab"; id: string } | { kind: "pane" } | null;

/**
 * What "close terminal / close tab" must act on — the thing the user is
 * looking at. An orphan tab overlaying a split wins over the split: closing
 * the active *pane* there would destroy the hidden split's terminals (e.g. a
 * tmux-shim agent swarm) instead of the visible tab. Otherwise a split closes
 * its active pane (which cancels a freshly-split empty pane), and a non-split
 * layout closes the active terminal tab.
 */
export function resolveCloseTarget(): CloseTarget {
	const orphanId = getActiveOrphanTabId();
	if (orphanId) return { kind: "tab", id: orphanId };
	if (paneLayoutStore.isSplit()) return { kind: "pane" };
	const activeId = terminalsStore.state.activeId;
	return activeId ? { kind: "tab", id: activeId } : null;
}

/**
 * The single "close terminal / close tab" action behind every implicit entry
 * point (keyboard, command palette, native menu, terminal context menu):
 * resolves what the user is looking at, then dispatches to the matching closer.
 * Entry points that name a specific tab (tab-strip and pane sub-tab menus) call
 * the tab closer directly and never come through here.
 */
export function closeActiveTarget(deps: { closeTab: (id: string) => unknown; closePane: () => void }): void {
	const target = resolveCloseTarget();
	if (target?.kind === "pane") deps.closePane();
	else if (target) deps.closeTab(target.id);
}
