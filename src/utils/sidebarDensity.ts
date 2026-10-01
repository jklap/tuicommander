import { type Accessor, createContext, createSignal, onCleanup, useContext } from "solid-js";

/** `compact` is the classic one-line rows; `rich` spends spare room on detail lines. */
export type SidebarDensity = "compact" | "rich";

/** The user's choice: `auto` derives the density, the others force it. */
export type SidebarDensityMode = "auto" | SidebarDensity;

const MODE_CYCLE: readonly SidebarDensityMode[] = ["auto", "compact", "rich"];

export const isSidebarDensityMode = (v: unknown): v is SidebarDensityMode =>
	MODE_CYCLE.includes(v as SidebarDensityMode);

/** Next mode of the toolbar toggle: auto, compact, rich, then auto again. */
export const nextSidebarDensityMode = (m: SidebarDensityMode): SidebarDensityMode =>
	MODE_CYCLE[(MODE_CYCLE.indexOf(m) + 1) % MODE_CYCLE.length];

/**
 * Below this many sidebar rows (repo headers + visible branch rows) the list is
 * short enough to give each row two or three lines: 12 rich rows at about 48px
 * still fit a 768px tablet viewport without scrolling.
 */
export const ROOMY_MAX_ROWS = 12;

export interface DensityRepoShape {
	collapsed: boolean;
	expanded: boolean;
	workspaces: Record<string, { terminals: readonly unknown[]; tabsCollapsed?: boolean }>;
}

/**
 * Rows the sidebar would render: one header per repo, one row per branch when the
 * repo is open, and one row per terminal tab under a branch whose tab list is shown
 * (`tabTreeEnabled` on and the branch not collapsed). Every open repo also renders
 * the sidebar plugin panels, `pluginRowsPerRepo` rows in all.
 */
export function countSidebarRows(
	repos: readonly DensityRepoShape[],
	tabTreeEnabled: boolean,
	pluginRowsPerRepo = 0,
): number {
	return repos.reduce((n, r) => {
		if (!r.expanded || r.collapsed) return n + 1;
		const branches = Object.values(r.workspaces);
		const tabs = tabTreeEnabled ? branches.reduce((t, w) => t + (w.tabsCollapsed ? 0 : w.terminals.length), 0) : 0;
		return n + 1 + branches.length + tabs + pluginRowsPerRepo;
	}, 0);
}

/**
 * A finger always gets the rich layout (its rows carry the 44px targets, see the
 * pointer media query in Sidebar.module.css), however many repos there are.
 * Otherwise a short list is shown rich and a long one compact. A forced mode wins.
 */
export function sidebarDensity(
	rowCount: number,
	coarsePointer: boolean,
	mode: SidebarDensityMode = "auto",
): SidebarDensity {
	if (mode !== "auto") return mode;
	if (coarsePointer) return "rich";
	return rowCount <= ROOMY_MAX_ROWS ? "rich" : "compact";
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

/** The density the enclosing sidebar resolved; rows read it instead of taking a prop per level. */
const COMPACT: Accessor<SidebarDensity> = () => "compact";
export const SidebarDensityContext = createContext<Accessor<SidebarDensity>>(COMPACT);
export const useSidebarDensity = (): Accessor<SidebarDensity> => useContext(SidebarDensityContext) ?? COMPACT;
