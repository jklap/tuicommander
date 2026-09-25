import { diffTabsStore } from "../stores/diffTabs";
import { editorTabsStore } from "../stores/editorTabs";
import { mdTabsStore } from "../stores/mdTabs";
import type { PaneTab } from "../stores/paneLayout";
import { terminalsStore } from "../stores/terminals";

/**
 * Whether `tab`'s underlying content still exists in its owning store. A tab
 * whose store entry is gone renders nothing (no tab strip entry, no close
 * button) — see `src/AGENTS.md`'s "`paneLayoutStore` Ghost Tabs..." section.
 * Used to decide whether a pane still has anything worth keeping open, rather
 * than trusting a raw `tabs.length` count that a dead entry can never let
 * reach zero.
 */
export function isPaneTabLive(tab: PaneTab): boolean {
	switch (tab.type) {
		case "terminal":
			return terminalsStore.get(tab.id) != null;
		case "diff":
			return diffTabsStore.get(tab.id) != null;
		case "markdown":
			return mdTabsStore.get(tab.id) != null;
		case "editor":
			return editorTabsStore.get(tab.id) != null;
	}
}
