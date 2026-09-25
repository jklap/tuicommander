import { createSignal } from "solid-js";
import { createStore, produce, reconcile } from "solid-js/store";
import { invoke } from "../invoke";
import { appLogger } from "./appLogger";

// ---- Layout Tree Types (geometry only) ----

/** Discriminated union for tree nodes */
export type PaneNode = PaneLeaf | PaneBranch;

/** Leaf node — references a PaneGroup by ID */
export interface PaneLeaf {
	type: "leaf";
	id: string;
}

/** Branch node — contains children split in a direction */
export interface PaneBranch {
	type: "branch";
	direction: "horizontal" | "vertical";
	children: PaneNode[];
	ratios: number[]; // length === children.length, sum === 1.0
}

// ---- Content Types (tabs per pane) ----

/** Tab types that can live inside a pane group */
export type PaneTabType = "terminal" | "markdown" | "diff" | "editor";

/** Reference to an existing tab in its respective store */
export interface PaneTab {
	id: string;
	type: PaneTabType;
}

/** A group of tabs rendered inside a single leaf pane */
export interface PaneGroup {
	id: string;
	tabs: PaneTab[];
	activeTabId: string | null;
}

// ---- Serializable State (for persistence) ----

export interface PaneLayoutState {
	root: PaneNode | null;
	groups: Record<string, PaneGroup>;
	activeGroupId: string | null;
}

// ---- Constants ----

/** Maximum nesting depth for recursive splits */
export const MAX_SPLIT_DEPTH = 3;

/** Minimum fraction a pane can occupy */
export const MIN_PANE_RATIO = 0.05;

// ---- Tree Utilities (operate on plain JS objects, no SolidJS proxies) ----

let groupCounter = 0;

/** Generate a unique group ID */
export function nextGroupId(): string {
	return `g${++groupCounter}`;
}

/** Reset counter (for tests) */
export function resetGroupCounter(): void {
	groupCounter = 0;
}

/** Calculate depth of a node in the tree (leaf = 0, branch = 1 + max child depth) */
export function nodeDepth(node: PaneNode): number {
	if (node.type === "leaf") return 0;
	return 1 + Math.max(...node.children.map(nodeDepth));
}

/** Find the depth at which a specific leaf sits (distance from root) */
export function leafDepthFromRoot(root: PaneNode, leafId: string): number {
	function walk(node: PaneNode, depth: number): number {
		if (node.type === "leaf") return node.id === leafId ? depth : -1;
		for (const child of node.children) {
			const found = walk(child, depth + 1);
			if (found >= 0) return found;
		}
		return -1;
	}
	return walk(root, 0);
}

/** Find the leaf node for a given group ID */
export function findLeaf(root: PaneNode, groupId: string): PaneLeaf | null {
	if (root.type === "leaf") return root.id === groupId ? root : null;
	for (const child of root.children) {
		const found = findLeaf(child, groupId);
		if (found) return found;
	}
	return null;
}

/** Find the parent branch of a node with the given group ID, and the child index */
export function findParent(root: PaneNode, targetId: string): { parent: PaneBranch; index: number } | null {
	if (root.type === "leaf") return null;
	for (let i = 0; i < root.children.length; i++) {
		const child = root.children[i];
		if (child.type === "leaf" && child.id === targetId) {
			return { parent: root, index: i };
		}
		if (child.type === "branch") {
			const found = findParent(child, targetId);
			if (found) return found;
		}
	}
	return null;
}

/** Collect all leaf IDs from the tree (DFS order) */
export function allLeafIds(node: PaneNode): string[] {
	if (node.type === "leaf") return [node.id];
	return node.children.flatMap(allLeafIds);
}

/** Normalize ratios so they sum to 1.0 */
export function normalizeRatios(ratios: number[]): number[] {
	const sum = ratios.reduce((a, b) => a + b, 0);
	if (sum === 0) return ratios.map(() => 1 / ratios.length);
	return ratios.map((r) => r / sum);
}

/**
 * Split a leaf into a branch with two children.
 * Returns a new tree (immutable) or null if max depth would be exceeded.
 */
export function splitLeaf(
	root: PaneNode,
	leafId: string,
	direction: "horizontal" | "vertical",
	newGroupId: string,
): PaneNode | null {
	const currentDepth = leafDepthFromRoot(root, leafId);
	if (currentDepth < 0) return null; // leaf not found
	if (currentDepth + 1 >= MAX_SPLIT_DEPTH) return null; // would exceed max depth

	return transformNode(root, leafId, (leaf) => ({
		type: "branch" as const,
		direction,
		children: [leaf, { type: "leaf" as const, id: newGroupId }],
		ratios: [0.5, 0.5],
	}));
}

/** Replace a leaf with a new node (used by splitLeaf). Returns new tree. */
function transformNode(node: PaneNode, targetLeafId: string, transform: (leaf: PaneLeaf) => PaneNode): PaneNode {
	if (node.type === "leaf") {
		return node.id === targetLeafId ? transform(node) : node;
	}
	const newChildren = node.children.map((child) => transformNode(child, targetLeafId, transform));
	if (newChildren.every((c, i) => c === node.children[i])) return node;
	return { ...node, children: newChildren };
}

/**
 * Remove a leaf from the tree and flatten single-child branches.
 * Returns the new root, or null if tree becomes empty.
 */
export function removeLeaf(root: PaneNode, leafId: string): PaneNode | null {
	if (root.type === "leaf") {
		return root.id === leafId ? null : root;
	}

	// Quick check: is the target leaf in this subtree at all?
	if (!allLeafIds(root).includes(leafId)) return root;

	const newChildren: PaneNode[] = [];
	const newRatios: number[] = [];

	for (let i = 0; i < root.children.length; i++) {
		const child = root.children[i];
		if (child.type === "leaf" && child.id === leafId) {
			continue;
		}
		if (child.type === "branch") {
			const result = removeLeaf(child, leafId);
			if (result) {
				newChildren.push(result);
				newRatios.push(root.ratios[i]);
			}
		} else {
			newChildren.push(child);
			newRatios.push(root.ratios[i]);
		}
	}

	if (newChildren.length === 0) return null;
	if (newChildren.length === 1) return newChildren[0];
	return { ...root, children: newChildren, ratios: normalizeRatios(newRatios) };
}

/**
 * Update a ratio at a resize handle position within a specific branch.
 * handleIndex is between children[handleIndex] and children[handleIndex+1].
 * fraction is the absolute position (0..1) within the branch's total space.
 */
export function setHandleRatio(branch: PaneBranch, handleIndex: number, fraction: number): PaneBranch {
	if (handleIndex < 0 || handleIndex >= branch.children.length - 1) return branch;

	const newRatios = [...branch.ratios];
	let offset = 0;
	for (let i = 0; i < handleIndex; i++) offset += newRatios[i];

	const combined = newRatios[handleIndex] + newRatios[handleIndex + 1];
	const splitPoint = Math.max(MIN_PANE_RATIO, Math.min(fraction - offset, combined - MIN_PANE_RATIO));

	newRatios[handleIndex] = splitPoint;
	newRatios[handleIndex + 1] = combined - splitPoint;

	return { ...branch, ratios: newRatios };
}

/**
 * Find the spatially adjacent leaf in a given direction.
 * Used for Alt+Arrow navigation in the tree.
 */
export function findAdjacentLeaf(
	root: PaneNode,
	fromLeafId: string,
	direction: "left" | "right" | "up" | "down",
): string | null {
	const axis = direction === "left" || direction === "right" ? "vertical" : "horizontal";
	const forward = direction === "right" || direction === "down";

	const path = pathToLeaf(root, fromLeafId);
	if (!path) return null;

	for (let i = path.length - 1; i >= 0; i--) {
		const { node, childIndex } = path[i];
		if (node.type !== "branch" || node.direction !== axis) continue;

		const targetIndex = forward ? childIndex + 1 : childIndex - 1;
		if (targetIndex < 0 || targetIndex >= node.children.length) continue;

		return forward ? firstLeaf(node.children[targetIndex]) : lastLeaf(node.children[targetIndex]);
	}
	return null;
}

/** Build the path from root to a leaf: array of { node, childIndex } entries */
function pathToLeaf(root: PaneNode, leafId: string): Array<{ node: PaneNode; childIndex: number }> | null {
	if (root.type === "leaf") return root.id === leafId ? [] : null;

	for (let i = 0; i < root.children.length; i++) {
		const sub = pathToLeaf(root.children[i], leafId);
		if (sub !== null) {
			return [{ node: root, childIndex: i }, ...sub];
		}
	}
	return null;
}

/** Get the first (leftmost/topmost) leaf ID in a subtree */
function firstLeaf(node: PaneNode): string {
	if (node.type === "leaf") return node.id;
	return firstLeaf(node.children[0]);
}

/** Get the last (rightmost/bottommost) leaf ID in a subtree */
function lastLeaf(node: PaneNode): string {
	if (node.type === "leaf") return node.id;
	return lastLeaf(node.children[node.children.length - 1]);
}

// ---- tmux `select-layout` bridging ----
//
// `tileLeaves`/`mainVertical` build a whole tree DIRECTLY as a literal and
// hand it to `setRoot()`, rather than reaching it via repeated `splitLeaf()`
// calls. This deliberately sidesteps `splitLeaf`'s `MAX_SPLIT_DEPTH` check
// (line ~143 above): that guard only bounds *incremental* single-step
// splitting (the interactive Cmd+\ path), and `setRoot()` itself enforces no
// depth limit at all — a leaf's own depth from the root is irrelevant to
// whether the tree can be *set*, only to whether it can be *split further*.
// A single row of N panes is depth 1; a 2D grid is depth 2 — both trivially
// fine either way.

/**
 * Arrange `ids` into a balanced grid, real tmux's own `tiled` layout.
 * `ids.length <= 1` returns a bare leaf (or `null` for zero ids) — no
 * branch node for a single pane.
 */
export function tileLeaves(ids: string[]): PaneNode | null {
	if (ids.length === 0) return null;
	if (ids.length === 1) return { type: "leaf", id: ids[0] };

	const cols = Math.ceil(Math.sqrt(ids.length));
	const rows = Math.ceil(ids.length / cols);
	const columns: string[][] = [];
	for (let c = 0; c < cols; c++) {
		const column = ids.slice(c * rows, c * rows + rows);
		if (column.length > 0) columns.push(column);
	}

	const toColumnNode = (column: string[]): PaneNode =>
		column.length === 1
			? { type: "leaf", id: column[0] }
			: {
					type: "branch",
					direction: "vertical",
					children: column.map((id) => ({ type: "leaf", id }) as PaneNode),
					ratios: normalizeRatios(column.map(() => 1)),
				};

	const children = columns.map(toColumnNode);
	if (children.length === 1) return children[0];
	return {
		type: "branch",
		direction: "horizontal",
		children,
		ratios: normalizeRatios(children.map(() => 1)),
	};
}

/**
 * Real tmux's `main-vertical`: the first id ("leader") keeps a fixed 30%
 * column (matching `resize-pane -x 30%`, which `TmuxBackend.rebalancePanesWithLeader`
 * always issues right after `select-layout main-vertical` — see Appendix A
 * of the tmux-shim plan), the rest stack vertically in the remaining 70%.
 * Unreachable from TUIC's environment today (only the "inside a real tmux
 * session" leader branch calls it, and TUIC deliberately never sets `$TMUX`)
 * — built alongside `tileLeaves` for forward-compat, at near-zero extra cost.
 */
export function mainVertical(ids: string[]): PaneNode | null {
	if (ids.length === 0) return null;
	if (ids.length === 1) return { type: "leaf", id: ids[0] };

	const [leader, ...rest] = ids;
	const restNode: PaneNode =
		rest.length === 1
			? { type: "leaf", id: rest[0] }
			: {
					type: "branch",
					direction: "vertical",
					children: rest.map((id) => ({ type: "leaf", id }) as PaneNode),
					ratios: normalizeRatios(rest.map(() => 1)),
				};
	return {
		type: "branch",
		direction: "horizontal",
		children: [{ type: "leaf", id: leader }, restNode],
		ratios: [0.3, 0.7],
	};
}

// ---- Store ----
// The tree (root) lives in plain JS to avoid SolidJS deep-proxy issues.
// Groups and activeGroupId live in a SolidJS store for fine-grained reactivity.
// A revision signal bumps on every tree mutation to trigger re-renders.

interface GroupsState {
	groups: Record<string, PaneGroup>;
	activeGroupId: string | null;
}

function createPaneLayoutStore() {
	// Plain JS tree — no proxy wrapping
	let tree: PaneNode | null = null;
	let restoredFromDisk = false;
	const [treeRevision, setTreeRevision] = createSignal(0);

	const [state, setState] = createStore<GroupsState>({
		groups: {},
		activeGroupId: null,
	});

	function bumpTree(): void {
		setTreeRevision((r) => r + 1);
	}

	// Debounced persistence — serializes tree+groups directly (no `this` needed)
	let saveTimer: ReturnType<typeof setTimeout> | null = null;

	function writeLayout(): void {
		const groups: Record<string, PaneGroup> = {};
		for (const key of Object.keys(state.groups)) {
			const g = state.groups[key];
			groups[key] = {
				id: g.id,
				tabs: g.tabs.map((t) => ({ id: t.id, type: t.type })),
				activeTabId: g.activeTabId,
			};
		}
		const snapshot: PaneLayoutState = {
			root: tree ? structuredClone(tree) : null,
			groups,
			activeGroupId: state.activeGroupId,
		};
		invoke("save_pane_layout", { layout: snapshot }).catch((err: unknown) =>
			appLogger.warn("app", "Failed to save pane layout", err),
		);
	}

	function scheduleSave(): void {
		if (saveTimer) clearTimeout(saveTimer);
		saveTimer = setTimeout(() => {
			saveTimer = null;
			writeLayout();
		}, 500);
	}

	const result = {
		state,

		/** Read the tree root. Call treeRevision() first to subscribe to changes. */
		getRoot(): PaneNode | null {
			treeRevision(); // subscribe to tree changes
			return tree;
		},

		/** Reactivity signal for tree changes */
		treeRevision,

		/** Create a new empty group and return its ID */
		createGroup(): string {
			const id = nextGroupId();
			setState("groups", id, { id, tabs: [], activeTabId: null });
			return id;
		},

		/** Remove a group from the groups map */
		removeGroup(groupId: string): void {
			setState(
				produce((s) => {
					delete s.groups[groupId];
					if (s.activeGroupId === groupId) {
						const remaining = tree ? allLeafIds(tree) : [];
						s.activeGroupId = remaining.find((id) => id !== groupId) ?? null;
					}
				}),
			);
		},

		/** Set the root of the layout tree */
		setRoot(root: PaneNode | null): void {
			tree = root;
			bumpTree();
		},

		/** Set the active group */
		setActiveGroup(groupId: string): void {
			if (state.groups[groupId]) {
				setState("activeGroupId", groupId);
			}
		},

		/** Split the given group's leaf in the specified direction */
		split(groupId: string, direction: "horizontal" | "vertical"): string | null {
			const newGroupId = this.createGroup();

			if (!tree) {
				tree = {
					type: "branch",
					direction,
					children: [
						{ type: "leaf", id: groupId },
						{ type: "leaf", id: newGroupId },
					],
					ratios: [0.5, 0.5],
				};
				bumpTree();
				setState("activeGroupId", newGroupId);
				scheduleSave();
				return newGroupId;
			}

			const newRoot = splitLeaf(tree, groupId, direction, newGroupId);
			if (!newRoot) {
				this.removeGroup(newGroupId);
				appLogger.warn("app", `Split rejected: max depth ${MAX_SPLIT_DEPTH} reached`);
				return null;
			}

			tree = newRoot;
			bumpTree();
			setState("activeGroupId", newGroupId);
			scheduleSave();
			return newGroupId;
		},

		/** Close a group's pane — removes from tree, flattens if needed */
		closePane(groupId: string): void {
			if (!tree) return;

			const newRoot = removeLeaf(tree, groupId);
			tree = newRoot;
			bumpTree();
			setState(
				produce((s) => {
					delete s.groups[groupId];
					if (s.activeGroupId === groupId) {
						s.activeGroupId = newRoot ? (allLeafIds(newRoot)[0] ?? null) : null;
					}
				}),
			);
			scheduleSave();
		},

		/** Add a tab to a group. `activate=false` docks the tab in the group's tab
		 *  strip without switching the pane to it — used by background spawns that
		 *  must not take over what the user is looking at. */
		addTab(groupId: string, tab: PaneTab, activate = true): void {
			const group = state.groups[groupId];
			if (!group) return;
			if (group.tabs.some((t) => t.id === tab.id && t.type === tab.type)) {
				if (activate) setState("groups", groupId, "activeTabId", tab.id);
				return;
			}
			setState(
				produce((s) => {
					s.groups[groupId].tabs.push(tab);
					if (activate || s.groups[groupId].activeTabId === null) {
						s.groups[groupId].activeTabId = tab.id;
					}
				}),
			);
			scheduleSave();
		},

		/** Remove a tab from a group */
		removeTab(groupId: string, tabId: string): void {
			setState(
				produce((s) => {
					const group = s.groups[groupId];
					if (!group) return;
					group.tabs = group.tabs.filter((t) => t.id !== tabId);
					if (group.activeTabId === tabId) {
						group.activeTabId = group.tabs.length > 0 ? group.tabs[group.tabs.length - 1].id : null;
					}
				}),
			);
			scheduleSave();
		},

		/** Move a tab from one group to another */
		moveTab(fromGroupId: string, toGroupId: string, tabId: string): void {
			const fromGroup = state.groups[fromGroupId];
			const toGroup = state.groups[toGroupId];
			if (!fromGroup || !toGroup) return;
			const tab = fromGroup.tabs.find((t) => t.id === tabId);
			if (!tab) return;

			setState(
				produce((s) => {
					const from = s.groups[fromGroupId];
					from.tabs = from.tabs.filter((t) => t.id !== tabId);
					if (from.activeTabId === tabId) {
						from.activeTabId = from.tabs.length > 0 ? from.tabs[from.tabs.length - 1].id : null;
					}
					const to = s.groups[toGroupId];
					if (!to.tabs.some((t) => t.id === tab.id && t.type === tab.type)) {
						to.tabs.push({ ...tab });
					}
					to.activeTabId = tabId;
				}),
			);
			scheduleSave();
		},

		/** Set the active tab within a group */
		setActiveTab(groupId: string, tabId: string): void {
			if (state.groups[groupId]) {
				setState("groups", groupId, "activeTabId", tabId);
			}
		},

		/** Navigate to adjacent pane in a direction */
		navigatePane(direction: "left" | "right" | "up" | "down"): string | null {
			if (!tree || !state.activeGroupId) return null;
			const targetId = findAdjacentLeaf(tree, state.activeGroupId, direction);
			if (targetId) {
				setState("activeGroupId", targetId);
			}
			return targetId;
		},

		/** Get the group for the active group ID */
		getActiveGroup(): PaneGroup | undefined {
			return state.activeGroupId ? state.groups[state.activeGroupId] : undefined;
		},

		/** Check if split mode is active */
		isSplit(): boolean {
			treeRevision(); // subscribe
			return tree !== null && tree.type === "branch";
		},

		/** Check if a group can be split (not at max depth) */
		canSplit(groupId: string): boolean {
			treeRevision(); // subscribe
			if (!tree) return false;
			const depth = leafDepthFromRoot(tree, groupId);
			return depth >= 0 && depth + 1 < MAX_SPLIT_DEPTH;
		},

		/** Get all group IDs in DFS order */
		getAllGroupIds(): string[] {
			treeRevision(); // subscribe
			return tree ? allLeafIds(tree) : [];
		},

		/** Find which group contains a given tab ID, or null if not found */
		getGroupForTab(tabId: string): string | null {
			treeRevision(); // subscribe
			for (const groupId of tree ? allLeafIds(tree) : []) {
				const group = state.groups[groupId];
				if (group?.tabs.some((t) => t.id === tabId)) return groupId;
			}
			return null;
		},

		/** Get all terminal tab IDs across all groups (ordered by group then tab position) */
		getTerminalTabIds(): string[] {
			const ids: string[] = [];
			for (const group of Object.values(state.groups)) {
				for (const tab of group.tabs) {
					if (tab.type === "terminal") ids.push(tab.id);
				}
			}
			return ids;
		},

		/**
		 * Arrange `sessionIds` (terminal tab ids, in the order they should
		 * appear) into a fresh split view for `layout` ("tiled" or
		 * "main-vertical") — the tmux compatibility shim's `select-layout`
		 * bridge (see `useAppInit.ts`'s `tmux-window-layout-requested`
		 * listener). Rebuilds the WHOLE layout from this call's id list,
		 * matching real tmux's own "tiled"/"main-vertical" semantics (they
		 * always re-tile everything, discarding the prior arrangement) —
		 * `select-layout tiled` arrives once per `split-window` in the live
		 * swarm flow, so each call already carries that window's current
		 * full pane list.
		 *
		 * SAFETY (code-review finding, 2026-09-15): a background tmux event
		 * must never destroy split-view state it doesn't own. Before
		 * touching anything, checks whether every group CURRENTLY in the
		 * split tree either already holds one of `sessionIds`, or the tree
		 * doesn't exist yet. If some other, unrelated group is also in the
		 * tree (the user's own manual split of unrelated terminals), this
		 * bails out entirely rather than clobbering it — the swarm's
		 * sessions simply stay whatever they already were (plain flat-view
		 * tabs, same as before this feature existed), which is the same
		 * "safe do less" degrade `closePane`'s "cancels an empty split
		 * instead of destroying the active terminal" already uses elsewhere
		 * in this file.
		 *
		 * A session already the SOLE tab of some group reuses that group
		 * (avoiding a duplicate tab reference — the exact invariant
		 * `paneLayout.test.ts`'s "pane tab identity" suite protects for the
		 * general case); a session sharing a group with other tabs, or not
		 * in any group yet, gets a fresh one. Any group left empty by a
		 * move is deleted, mirroring `closePane`'s own cleanup. The
		 * previously-active group is preserved if it's still one of the
		 * groups being arranged (the user is looking at one of these
		 * sessions already) — only falls back to the first arranged group
		 * when there was no sensible focus to preserve.
		 */
		arrangeSessionsAsLayout(sessionIds: string[], layout: string): void {
			const ids = [...new Set(sessionIds)].filter(Boolean);
			if (ids.length === 0) return;

			const existingLeafGroupIds = tree ? allLeafIds(tree) : [];
			const ownedGroupIds = new Set(ids.map((id) => this.getGroupForTab(id)).filter((g): g is string => g !== null));
			const hasUnrelatedGroupInTree = existingLeafGroupIds.some((g) => !ownedGroupIds.has(g));
			if (hasUnrelatedGroupInTree) {
				appLogger.warn(
					"app",
					"tmux select-layout request skipped: the current split view has other panes " +
						"this swarm doesn't own — leaving the split as-is rather than replacing it.",
				);
				return;
			}

			const groupIds: string[] = [];
			const emptiedGroupIds = new Set<string>();
			for (const sessionId of ids) {
				const existingGroupId = this.getGroupForTab(sessionId);
				const existingGroup = existingGroupId ? state.groups[existingGroupId] : null;
				if (existingGroup && existingGroup.tabs.length === 1) {
					groupIds.push(existingGroupId as string);
					continue;
				}
				const newGroupId = this.createGroup();
				if (existingGroupId) {
					this.moveTab(existingGroupId, newGroupId, sessionId);
					if (state.groups[existingGroupId]?.tabs.length === 0) {
						emptiedGroupIds.add(existingGroupId);
					}
				} else {
					this.addTab(newGroupId, { id: sessionId, type: "terminal" });
				}
				groupIds.push(newGroupId);
			}

			const newRoot = layout === "main-vertical" ? mainVertical(groupIds) : tileLeaves(groupIds);
			tree = newRoot;
			bumpTree();
			setState(
				produce((s) => {
					for (const emptied of emptiedGroupIds) delete s.groups[emptied];
					if (!s.activeGroupId || !groupIds.includes(s.activeGroupId)) {
						s.activeGroupId = groupIds[0] ?? s.activeGroupId;
					}
				}),
			);
			scheduleSave();
		},

		/** Serialize layout for persistence (JSON-safe, no proxies involved) */
		serialize(): PaneLayoutState {
			// tree is plain JS, groups need to be cloned from SolidJS store
			const groups: Record<string, PaneGroup> = {};
			for (const key of Object.keys(state.groups)) {
				const g = state.groups[key];
				groups[key] = {
					id: g.id,
					tabs: g.tabs.map((t) => ({ id: t.id, type: t.type })),
					activeTabId: g.activeTabId,
				};
			}
			return {
				root: tree ? structuredClone(tree) : null,
				groups,
				activeGroupId: state.activeGroupId,
			};
		},

		/** Restore layout from serialized state */
		restore(saved: PaneLayoutState): void {
			const allIds = saved.root ? allLeafIds(saved.root) : [];
			const maxNum = allIds.reduce((max, id) => {
				const n = parseInt(id.replace("g", ""), 10);
				return Number.isNaN(n) ? max : Math.max(max, n);
			}, 0);
			groupCounter = maxNum;

			tree = saved.root;
			bumpTree();
			// reconcile, not a wholesale node replace: `saved.groups` is always a
			// fresh object, so a plain setState would hand every group and every
			// PaneTab a new proxy and remount every CanvasTerminal in the layout.
			setState(
				reconcile({
					groups: saved.groups,
					activeGroupId: saved.activeGroupId,
				}),
			);
		},

		/** Remap terminal IDs in all groups (used after lazy restore creates new terminal IDs) */
		remapTerminalIds(idMap: Map<string, string>): void {
			setState(
				produce((s) => {
					for (const group of Object.values(s.groups)) {
						for (const tab of group.tabs) {
							const newId = idMap.get(tab.id);
							if (newId) tab.id = newId;
						}
						if (group.activeTabId && idMap.has(group.activeTabId)) {
							group.activeTabId = idMap.get(group.activeTabId)!;
						}
					}
				}),
			);
			scheduleSave();
		},

		/** Reset to single pane (no split) */
		reset(): void {
			// Every branch select calls this, almost always on an already-empty
			// layout. Writing a fresh `{}` would replace the `groups` node, bump
			// treeRevision for every isSplit() subscriber and rewrite
			// pane-layout.json with identical bytes — all for no change.
			if (tree === null && state.activeGroupId === null && Object.keys(state.groups).length === 0) {
				restoredFromDisk = false;
				return;
			}
			tree = null;
			restoredFromDisk = false;
			bumpTree();
			setState({
				groups: {},
				activeGroupId: null,
			});
			scheduleSave();
		},

		/** Returns true (once) if layout was loaded from disk and not yet consumed */
		consumeRestoredFromDisk(): boolean {
			const was = restoredFromDisk;
			restoredFromDisk = false;
			return was;
		},

		/** Load pane layout from disk and restore (filtering non-terminal tabs) */
		async loadFromDisk(): Promise<void> {
			try {
				const saved = await invoke<PaneLayoutState | null>("load_pane_layout");
				if (!saved?.root) return;

				// Filter out non-terminal tabs (md/diff/editor are session-bound)
				for (const group of Object.values(saved.groups)) {
					group.tabs = group.tabs.filter((t: PaneTab) => t.type === "terminal");
					if (group.activeTabId && !group.tabs.some((t: PaneTab) => t.id === group.activeTabId)) {
						group.activeTabId = group.tabs.length > 0 ? group.tabs[0].id : null;
					}
				}

				this.restore(saved);
				restoredFromDisk = true;
				appLogger.info("app", `Pane layout restored: ${allLeafIds(saved.root).length} panes`);
			} catch (err) {
				appLogger.warn("app", "Failed to load pane layout", err);
			}
		},
	};

	return {
		...result,
		/** Write a pending debounced save now — the app is going away. */
		flushSave(): void {
			if (!saveTimer) return;
			clearTimeout(saveTimer);
			saveTimer = null;
			writeLayout();
		},
		_testCancelPendingSave(): void {
			if (saveTimer) {
				clearTimeout(saveTimer);
				saveTimer = null;
			}
		},
	};
}

export const paneLayoutStore = createPaneLayoutStore();

// Debug registry — expose layout tree for MCP introspection
import { registerDebugSnapshot } from "./debugRegistry";

registerDebugSnapshot("paneLayout", () => {
	const s = paneLayoutStore.state;
	return {
		activeGroupId: s.activeGroupId,
		root: paneLayoutStore.getRoot(),
		groups: Object.fromEntries(
			Object.entries(s.groups).map(([id, g]) => [
				id,
				{
					tabs: g.tabs,
					activeTabId: g.activeTabId,
				},
			]),
		),
	};
});

// Sweep a terminal's tab out of the layout the moment it's actually removed,
// collapsing the pane if nothing live is left. Closes a gap the explicit
// tab-close path (useTerminalLifecycle.ts's removeTabFromPane) can't reach on
// its own: a pane whose ONLY tab is a ghost (its session died via a path that
// never fired session-closed — e.g. a tmux-shim-materialized pane killed
// outside the normal close route) has no live sibling tab to ever trigger a
// close, so it stays wedged open until something unrelated happens to close a
// tab in that same group. This mirrors globalWorkspace.ts's own
// terminalsStore.onRemove wiring for the identical reason. See
// src/AGENTS.md's "paneLayoutStore Ghost Tabs..." section.
import { isPaneTabLive } from "../utils/paneTabLiveness";
import { terminalsStore } from "./terminals";

terminalsStore.onRemove((id) => {
	const groupId = paneLayoutStore.getGroupForTab(id);
	if (!groupId) return;
	paneLayoutStore.removeTab(groupId, id);
	const updated = paneLayoutStore.state.groups[groupId];
	if (!updated?.tabs.some(isPaneTabLive)) {
		paneLayoutStore.closePane(groupId);
	}
});
