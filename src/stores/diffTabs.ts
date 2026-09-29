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
}

/** Whether a diff tab is the Session Diff Review surface. */
export function isSessionReviewTab(tab: DiffTabData | undefined): boolean {
	return tab?.scope === SESSION_SCOPE;
}

function createDiffTabsStore() {
	const base = createTabManager<DiffTabData>("diff");
	const handles = new Map<string, unknown>();

	return {
		state: base.state,
		remove: base.remove,
		setActive: base.setActive,
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
				base.setActive(existing.id);
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
		addSessionReview(repoPath: string, sessionId?: string): string {
			const existing = Object.values(base.state.tabs).find(
				(tab) => tab.repoPath === repoPath && tab.scope === SESSION_SCOPE,
			);
			if (existing) {
				base.setActive(existing.id);
				if (sessionId) base._setState("tabs", existing.id, "sessionId", sessionId);
				return existing.id;
			}

			const id = base._nextId("diff");
			return base._addTab({
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
