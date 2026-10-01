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
	workspaces: Record<string, unknown>;
}

/** Rows the sidebar would render: one header per repo plus its branch rows when open. */
export function countSidebarRows(repos: readonly DensityRepoShape[]): number {
	return repos.reduce((n, r) => n + 1 + (r.expanded && !r.collapsed ? Object.keys(r.workspaces).length : 0), 0);
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
