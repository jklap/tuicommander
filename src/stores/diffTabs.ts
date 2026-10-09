import { pathBasename } from "../utils/pathUtils";
import { branchKeyFor } from "./repositories";
import { type BaseTab, createTabManager } from "./tabManager";

export type DiffStatus = "M" | "A" | "D" | "R" | "?";

const VALID_DIFF_STATUSES = new Set<string>(["M", "A", "D", "R", "?"]);

/** Type guard for DiffStatus values received from backend */
export function isDiffStatus(value: unknown): value is DiffStatus {
	return typeof value === "string" && VALID_DIFF_STATUSES.has(value);
}

/** Sentinel `scope` marking a diff tab as the Session Diff Review surface —
 *  distinct from `undefined` (the "Diff Scroll" all-files view), `"staged"`,
 *  or a commit hash, so it can never collide with any of those. */
export const SESSION_SCOPE = "session";

/** Diff tab data */
export interface DiffTabData extends BaseTab {
	repoPath: string;
	filePath: string;
	fileName: string; // Display name (basename of filePath)
	status: DiffStatus;
	scope?: string; // "working" (default), "staged", a commit hash, or SESSION_SCOPE
	untracked?: boolean; // True for "?" status files — skips redundant ls-files probe
	/** Only set when scope === SESSION_SCOPE. Mutable — the in-tab session
	 *  picker writes back into it so the choice survives remount/detach. */
	sessionId?: string;
	/** Session Diff Review only: a `session-review-changed` event arrived for
	 *  this tab's session while it wasn't the active tab. Cleared the moment
	 *  the tab becomes active (see `setActive` below) — mirrors
	 *  `terminalsStore`'s own `unseen` field/clear-on-activate pattern. */
	unseen?: boolean;
}

/** Whether a diff tab is the Session Diff Review surface. */
export function isSessionReviewTab(tab: DiffTabData | undefined): boolean {
	return tab?.scope === SESSION_SCOPE;
}

function createDiffTabsStore() {
	const base = createTabManager<DiffTabData>("diff");
	const handles = new Map<string, unknown>();

	/** Clears `unseen` (if set) before delegating to `base.setActive` — mirrors
	 *  `terminalsStore.setActive`'s own inline clear for its `unseen` field.
	 *  Shared by every path that activates a diff tab, not just the public
	 *  `setActive` passthrough — `add()`/`addSessionReview()`'s "tab already
	 *  exists, focus it" branches must clear it too. */
	function setActiveAndClearUnseen(id: string): void {
		if (base.state.tabs[id]?.unseen) {
			base._setState("tabs", id, "unseen", false);
		}
		base.setActive(id);
	}

	return {
		state: base.state,
		remove: base.remove,
		setActive(id: string | null): void {
			if (id) setActiveAndClearUnseen(id);
			else base.setActive(id);
		},
		clearAll: base.clearAll,
		get: base.get,
		getIds: base.getIds,
		getVisibleIds: base.getVisibleIds,
		getActive: base.getActive,
		getCount: base.getCount,
		setPinned: base.setPinned,
		reorderByIds: base.reorderByIds,

		/** Add a new diff tab (or return existing if same file+scope already open).
		 *  Deactivates terminal/md/editor tabs so the diff pane becomes visible. */
		add(repoPath: string, filePath: string, status: DiffStatus, scope?: string, untracked?: boolean): string {
			const existing = Object.values(base.state.tabs).find(
				(tab) => tab.repoPath === repoPath && tab.filePath === filePath && tab.scope === scope,
			);
			if (existing) {
				setActiveAndClearUnseen(existing.id);
				return existing.id;
			}

			const id = base._nextId("diff");
			const fileName = filePath ? pathBasename(filePath) || filePath : "Diff Scroll";
			const tabId = base._addTab({
				id,
				repoPath,
				filePath,
				fileName,
				status,
				scope,
				untracked,
				branchKey: branchKeyFor(repoPath),
			});
			return tabId;
		},

		/** Open (or focus) the Session Diff Review tab for a repo — one per repo;
		 *  switching sessions happens inside the tab via its own picker, not by
		 *  opening more tabs. A sibling method to `add()` rather than an overload:
		 *  the dedupe key (repo + SESSION_SCOPE only, ignoring filePath) and the
		 *  `fileName` derivation both differ from `add()`'s, which already has
		 *  7 call sites depending on its existing contract. */
		addSessionReview(repoPath: string, sessionId?: string, activate = true): string {
			const existing = Object.values(base.state.tabs).find(
				(tab) => tab.repoPath === repoPath && tab.scope === SESSION_SCOPE,
			);
			if (existing) {
				if (activate) setActiveAndClearUnseen(existing.id);
				if (sessionId) base._setState("tabs", existing.id, "sessionId", sessionId);
				return existing.id;
			}

			const id = base._nextId("diff");
			return (activate ? base._addTab : base._addTabBackground)({
				id,
				repoPath,
				filePath: "",
				fileName: "Session Review",
				status: "M",
				scope: SESSION_SCOPE,
				sessionId,
				branchKey: branchKeyFor(repoPath),
			});
		},

		/** Persist the in-tab session picker's choice, so it survives remount/detach. */
		setSessionId(tabId: string, sessionId: string): void {
			base._setState("tabs", tabId, "sessionId", sessionId);
		},

		/** Called on a `session-review-changed` event: flags the matching, open,
		 *  currently-INACTIVE Session Diff tab as having unseen content. A no-op
		 *  when the tab is already active (the user is looking at it) or when
		 *  no tab for this repo+session is even open. */
		markSessionReviewUnseen(repoPath: string, sessionId: string): void {
			const tab = Object.values(base.state.tabs).find(
				(t) => t.repoPath === repoPath && t.scope === SESSION_SCOPE && t.sessionId === sessionId,
			);
			if (!tab || tab.id === base.state.activeId) return;
			base._setState("tabs", tab.id, "unseen", true);
		},

		/** Register an imperative handle for a tab (e.g. openSearch) */
		setHandle(tabId: string, handle: unknown): void {
			handles.set(tabId, handle);
		},

		/** Remove the imperative handle when a tab component unmounts */
		clearHandle(tabId: string): void {
			handles.delete(tabId);
		},

		/** Retrieve the imperative handle for a tab */
		getHandle<T = unknown>(tabId: string): T | undefined {
			return handles.get(tabId) as T | undefined;
		},

		/** Clear all diff tabs for a repository */
		clearForRepo(repoPath: string): void {
			base._clearWhere((tab) => tab.repoPath === repoPath);
		},

		/** Get tabs for a specific repository */
		getForRepo(repoPath: string): DiffTabData[] {
			return Object.values(base.state.tabs).filter((tab) => tab.repoPath === repoPath);
		},
	};
}

export const diffTabsStore = createDiffTabsStore();
