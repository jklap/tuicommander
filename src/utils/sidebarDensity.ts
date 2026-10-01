import { type Accessor, createSignal, onCleanup } from "solid-js";

export type SidebarDensity = "compact" | "comfortable" | "touch";

/** The user's choice: `auto` derives the density, the others force it. */
export type SidebarDensityMode = "auto" | SidebarDensity;

const MODE_CYCLE: readonly SidebarDensityMode[] = ["auto", "compact", "comfortable", "touch"];

export const isSidebarDensityMode = (v: unknown): v is SidebarDensityMode =>
	MODE_CYCLE.includes(v as SidebarDensityMode);

/** Next mode of the toolbar toggle: auto, compact, comfortable, touch, then auto again. */
export const nextSidebarDensityMode = (m: SidebarDensityMode): SidebarDensityMode =>
	MODE_CYCLE[(MODE_CYCLE.indexOf(m) + 1) % MODE_CYCLE.length];

/**
 * Below this many sidebar rows (repo headers + visible branch rows) the list is
 * short enough to breathe: 16 rows at the comfortable 30px still fit a 768px
 * tablet viewport without scrolling.
 */
export const ROOMY_MAX_ROWS = 16;

export interface DensityRepoShape {
	collapsed: boolean;
	expanded: boolean;
	workspaces: Record<string, { terminals: readonly unknown[]; tabsCollapsed?: boolean }>;
}

/**
 * Rows the sidebar would render: one header per repo, one row per branch when the
 * repo is open, and one row per terminal tab under a branch whose tab list is shown
 * (`tabTreeEnabled` on and the branch not collapsed).
 */
export function countSidebarRows(repos: readonly DensityRepoShape[], tabTreeEnabled: boolean): number {
	return repos.reduce((n, r) => {
		if (!r.expanded || r.collapsed) return n + 1;
		const branches = Object.values(r.workspaces);
		const tabs = tabTreeEnabled ? branches.reduce((t, w) => t + (w.tabsCollapsed ? 0 : w.terminals.length), 0) : 0;
		return n + 1 + branches.length + tabs;
	}, 0);
}

/**
 * Touch wins over row count: a finger needs 44px targets (Apple HIG) however
 * many repos there are. Otherwise a short list is given more room. A forced
 * mode wins over both.
 */
export function sidebarDensity(
	rowCount: number,
	coarsePointer: boolean,
	mode: SidebarDensityMode = "auto",
): SidebarDensity {
	if (mode !== "auto") return mode;
	if (coarsePointer) return "touch";
	return rowCount <= ROOMY_MAX_ROWS ? "comfortable" : "compact";
}

const COARSE_QUERY = "(pointer: coarse)";

/** Live `(pointer: coarse)` — true when the primary input is a finger. */
export function createCoarsePointer(): Accessor<boolean> {
	const mql = typeof window !== "undefined" ? window.matchMedia?.(COARSE_QUERY) : undefined;
	const [coarse, setCoarse] = createSignal(mql?.matches ?? false);
	if (mql) {
		const onChange = (e: MediaQueryListEvent) => setCoarse(e.matches);
		mql.addEventListener?.("change", onChange);
		onCleanup(() => mql.removeEventListener?.("change", onChange));
	}
	return coarse;
}
