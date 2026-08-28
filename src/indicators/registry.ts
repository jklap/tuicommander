import { type AnimationId, isKnownAnimationId } from "./animations";
import { type IconId, isKnownIconId } from "./icons";

export type IndicatorGroup = "terminalStatus" | "tabType" | "sidebarSymbol" | "prBadge" | "gitState" | "diffStat";

export type IndicatorCapability = "color" | "icon" | "animation";

/**
 * A user override of one registry entry's default color/icon/animation.
 * Persisted verbatim in `AppConfig.indicator_overrides` (config.rs) —
 * field names match the Rust struct exactly (single words, no camelCase),
 * so there's no wire-format mapper needed on either side.
 */
export interface IndicatorOverride {
	id: string;
	color?: string;
	icon?: string;
	animation?: string;
}

export interface IndicatorDef {
	/** Stable, persisted id — e.g. "terminal.busy". Never rename once shipped;
	 *  a saved override references this string. */
	id: string;
	group: IndicatorGroup;
	label: string;
	description: string;
	/** CSS custom property the resolved color is published as. Absent when
	 *  the indicator has no independent color of its own (e.g. sidebarSymbol's
	 *  "shell", whose color follows a different indicator's cascade). */
	colorVar?: string;
	/** Default value for colorVar — a `var()` token reference, NEVER a raw
	 *  hex, so a theme switch keeps flowing through untouched. Exception:
	 *  tabType's *-rgb vars, which are raw comma-separated triples (consumed
	 *  inside `rgba()`), also always defined via var() indirection to the
	 *  token they replace. */
	defaultColor?: string;
	/** CSS custom property the resolved `animation` shorthand is published as. */
	animVar?: string;
	defaultAnimation?: AnimationId;
	/** Which animations this indicator's picker offers. Omitted means "the
	 *  full set" — narrowed for shapes where most choices don't read as
	 *  intended (e.g. `glow` needs a small filled dot, not a bar). */
	animations?: readonly AnimationId[];
	defaultIconId?: IconId;
	capabilities: readonly IndicatorCapability[];
	/** Preview shape the legend renders for this row. */
	preview: "dot" | "bar" | "symbol" | "badge";
}

const DOT_ANIMATIONS: readonly AnimationId[] = ["none", "pulse", "pulse-slow", "blink", "breathe", "glow"];
const BADGE_ANIMATIONS: readonly AnimationId[] = ["none", "pulse", "pulse-slow", "breathe"];

/**
 * The single source of truth for every customizable visual indicator in the
 * app. The UI Legend (`src/components/HelpPanel/UiLegend.tsx`) renders
 * FROM this list — it no longer hand-maintains its own copy of colors and
 * labels, which is what let it drift from the real components (see the
 * plan's Context section for the specific mismatches this fixed).
 *
 * `src/__tests__/indicators/registryParity.test.ts` enforces that every
 * `--ind-*`/`--ind-anim-*` var named here has a matching `:root` default in
 * `global.css`, and that the CSS files below don't still hardcode a core
 * palette var in place of their `--ind-*` var.
 */
export const INDICATORS: readonly IndicatorDef[] = [
	// -------------------------------------------------------------------
	// Terminal Status Dots — src/components/TabBar/TabBar.module.css
	// (also feeds the sidebar branch icon and nested-tab dot's COLOR only;
	// see sidebarSymbol below for why those don't get their own ids)
	// -------------------------------------------------------------------
	{
		id: "terminal.none",
		group: "terminalStatus",
		label: "No session",
		description: "Terminal never ran or was reset",
		colorVar: "--ind-terminal-none",
		defaultColor: "var(--fg-muted)",
		animVar: "--ind-anim-terminal-none",
		defaultAnimation: "none",
		defaultIconId: "dot",
		capabilities: ["color", "icon", "animation"],
		animations: DOT_ANIMATIONS,
		preview: "dot",
	},
	{
		id: "terminal.busy",
		group: "terminalStatus",
		label: "Busy",
		description: "Producing output",
		colorVar: "--ind-terminal-busy",
		defaultColor: "var(--activity)",
		animVar: "--ind-anim-terminal-busy",
		defaultAnimation: "pulse",
		defaultIconId: "dot",
		capabilities: ["color", "icon", "animation"],
		animations: DOT_ANIMATIONS,
		preview: "dot",
	},
	{
		id: "terminal.idle",
		group: "terminalStatus",
		label: "Idle",
		description: "Agent waiting, no recent output",
		colorVar: "--ind-terminal-idle",
		defaultColor: "var(--success)",
		animVar: "--ind-anim-terminal-idle",
		defaultAnimation: "none",
		defaultIconId: "dot",
		capabilities: ["color", "icon", "animation"],
		animations: DOT_ANIMATIONS,
		preview: "dot",
	},
	{
		id: "terminal.unseen",
		group: "terminalStatus",
		label: "Unseen",
		description: "Went idle while not viewed",
		colorVar: "--ind-terminal-unseen",
		defaultColor: "var(--unseen)",
		animVar: "--ind-anim-terminal-unseen",
		defaultAnimation: "none",
		defaultIconId: "dot",
		capabilities: ["color", "icon", "animation"],
		animations: DOT_ANIMATIONS,
		preview: "dot",
	},
	{
		id: "terminal.exited",
		group: "terminalStatus",
		label: "Exited",
		description: "Shell process exited",
		colorVar: "--ind-terminal-exited",
		defaultColor: "var(--fg-muted)",
		animVar: "--ind-anim-terminal-exited",
		defaultAnimation: "none",
		defaultIconId: "dot",
		capabilities: ["color", "icon", "animation"],
		animations: DOT_ANIMATIONS,
		preview: "dot",
	},
	{
		id: "terminal.question",
		group: "terminalStatus",
		label: "Question",
		description: "Agent needs input",
		colorVar: "--ind-terminal-question",
		defaultColor: "var(--attention)",
		animVar: "--ind-anim-terminal-question",
		defaultAnimation: "pulse",
		defaultIconId: "dot",
		capabilities: ["color", "icon", "animation"],
		animations: DOT_ANIMATIONS,
		preview: "dot",
	},
	{
		id: "terminal.error",
		group: "terminalStatus",
		label: "Error",
		description: "API error or agent stuck",
		colorVar: "--ind-terminal-error",
		defaultColor: "var(--error)",
		animVar: "--ind-anim-terminal-error",
		defaultAnimation: "pulse",
		defaultIconId: "dot",
		capabilities: ["color", "icon", "animation"],
		animations: DOT_ANIMATIONS,
		preview: "dot",
	},

	// -------------------------------------------------------------------
	// Tab Types — background tint + bottom border, TabBar.module.css /
	// PaneTree.css. Raw comma-separated RGB triples (consumed inside
	// rgba()), not a `var()`-of-a-color — see IndicatorDef.defaultColor.
	// -------------------------------------------------------------------
	{
		id: "tabType.diff",
		group: "tabType",
		label: "Diff",
		description: "Git diff viewer",
		colorVar: "--ind-tabtype-diff-rgb",
		defaultColor: "var(--tab-diff-rgb)",
		capabilities: ["color"],
		preview: "bar",
	},
	{
		id: "tabType.editor",
		group: "tabType",
		label: "Editor",
		description: "Code editor",
		colorVar: "--ind-tabtype-editor-rgb",
		defaultColor: "var(--tab-edit-rgb)",
		capabilities: ["color"],
		preview: "bar",
	},
	{
		id: "tabType.markdown",
		group: "tabType",
		label: "Markdown",
		description: "Markdown viewer",
		colorVar: "--ind-tabtype-markdown-rgb",
		defaultColor: "var(--tab-md-rgb)",
		capabilities: ["color"],
		preview: "bar",
	},
	{
		id: "tabType.panel",
		group: "tabType",
		label: "Panel",
		description: "Dashboard / plugin panel",
		colorVar: "--ind-tabtype-panel-rgb",
		defaultColor: "var(--tab-panel-rgb)",
		capabilities: ["color"],
		preview: "bar",
	},
	{
		id: "tabType.html",
		group: "tabType",
		label: "HTML Preview",
		description: "Rendered HTML file preview",
		colorVar: "--ind-tabtype-html-rgb",
		defaultColor: "var(--tab-html-rgb)",
		capabilities: ["color"],
		preview: "bar",
	},
	{
		id: "tabType.remote",
		group: "tabType",
		label: "PTY",
		description: "Remote session (HTTP/MCP)",
		colorVar: "--ind-tabtype-remote-rgb",
		defaultColor: "var(--tab-remote-rgb)",
		capabilities: ["color"],
		preview: "bar",
	},

	// -------------------------------------------------------------------
	// Sidebar Symbols — src/components/Sidebar/Sidebar.module.css BranchIcon.
	// The dynamic busy/question/error/unseen states on the branch icon are
	// the SAME semantic state as the terminalStatus entries above and
	// deliberately reuse those --ind-terminal-* vars rather than duplicating
	// them — one override then applies everywhere that state is shown.
	// -------------------------------------------------------------------
	{
		id: "sidebar.main",
		group: "sidebarSymbol",
		label: "Main branch",
		description: "Primary branch (main/master), checked out",
		colorVar: "--ind-sidebar-main",
		defaultColor: "var(--warning)",
		defaultIconId: "star",
		capabilities: ["color", "icon"],
		preview: "symbol",
	},
	{
		id: "sidebar.worktree",
		group: "sidebarSymbol",
		label: "Linked worktree",
		description: "A separate worktree, checked out",
		colorVar: "--ind-sidebar-worktree",
		defaultColor: "var(--success)",
		defaultIconId: "worktreeFork",
		capabilities: ["color", "icon"],
		preview: "symbol",
	},
	{
		id: "sidebar.branch",
		group: "sidebarSymbol",
		label: "Other branch",
		description: "A non-main branch checked out in the main worktree",
		colorVar: "--ind-sidebar-branch",
		defaultColor: "var(--accent)",
		defaultIconId: "branchArrow",
		capabilities: ["color", "icon"],
		preview: "symbol",
	},
	{
		id: "sidebar.shell",
		group: "sidebarSymbol",
		label: "Shell (non-git)",
		description: "A plain shell, not a git branch — color follows its own state",
		defaultIconId: "shellTerminal",
		capabilities: ["icon"],
		preview: "symbol",
	},
	{
		id: "sidebar.idle",
		group: "sidebarSymbol",
		label: "No open terminal",
		description: "Branch has no terminal open right now",
		colorVar: "--ind-sidebar-idle",
		defaultColor: "var(--fg-muted)",
		capabilities: ["color"],
		preview: "symbol",
	},
	{
		id: "sidebar.merged",
		group: "sidebarSymbol",
		label: "Merged badge",
		description: "Branch fully merged into main",
		colorVar: "--ind-sidebar-merged",
		defaultColor: "var(--fg-muted)",
		capabilities: ["color"],
		preview: "badge",
	},
	{
		id: "sidebar.remote",
		group: "sidebarSymbol",
		label: "Remote badge",
		description: "Terminal created via HTTP/MCP",
		colorVar: "--ind-sidebar-remote",
		defaultColor: "var(--fg-muted)",
		capabilities: ["color"],
		preview: "badge",
	},

	// -------------------------------------------------------------------
	// PR Status Badges — src/components/Sidebar/PrStateBadge.tsx.
	// `checking` gets its own class/var (was sharing `.prCiPending` with
	// `ci-pending` — split so each is independently customizable).
	// -------------------------------------------------------------------
	{
		id: "pr.open",
		group: "prBadge",
		label: "Open PR",
		description: "Open PR (number)",
		colorVar: "--ind-pr-open",
		defaultColor: "var(--success)",
		capabilities: ["color"],
		preview: "badge",
	},
	{
		id: "pr.ready",
		group: "prBadge",
		label: "Ready",
		description: "Approved and mergeable",
		colorVar: "--ind-pr-ready",
		defaultColor: "var(--success)",
		capabilities: ["color"],
		preview: "badge",
	},
	{
		id: "pr.draft",
		group: "prBadge",
		label: "Draft",
		description: "PR is a draft",
		colorVar: "--ind-pr-draft",
		defaultColor: "var(--fg-secondary)",
		capabilities: ["color"],
		preview: "badge",
	},
	{
		id: "pr.conflict",
		group: "prBadge",
		label: "Conflicts",
		description: "Merge conflicts",
		colorVar: "--ind-pr-conflict",
		defaultColor: "var(--error)",
		animVar: "--ind-anim-pr-conflict",
		defaultAnimation: "pulse",
		capabilities: ["color", "animation"],
		animations: BADGE_ANIMATIONS,
		preview: "badge",
	},
	{
		id: "pr.ci-failed",
		group: "prBadge",
		label: "CI Failed",
		description: "CI checks failed",
		colorVar: "--ind-pr-ci-failed",
		defaultColor: "var(--error)",
		capabilities: ["color"],
		preview: "badge",
	},
	{
		id: "pr.changes-requested",
		group: "prBadge",
		label: "Changes Req.",
		description: "Changes requested",
		colorVar: "--ind-pr-changes-requested",
		defaultColor: "var(--changes)",
		capabilities: ["color"],
		preview: "badge",
	},
	{
		id: "pr.review-required",
		group: "prBadge",
		label: "Review Req.",
		description: "Awaiting review",
		colorVar: "--ind-pr-review-required",
		defaultColor: "var(--changes)",
		capabilities: ["color"],
		preview: "badge",
	},
	{
		id: "pr.checking",
		group: "prBadge",
		label: "Checking",
		description: "GitHub is recomputing mergeability",
		colorVar: "--ind-pr-checking",
		defaultColor: "var(--changes)",
		animVar: "--ind-anim-pr-checking",
		defaultAnimation: "pulse-slow",
		capabilities: ["color", "animation"],
		animations: BADGE_ANIMATIONS,
		preview: "badge",
	},
	{
		id: "pr.ci-pending",
		group: "prBadge",
		label: "CI Running",
		description: "CI in progress",
		colorVar: "--ind-pr-ci-pending",
		defaultColor: "var(--changes)",
		animVar: "--ind-anim-pr-ci-pending",
		defaultAnimation: "pulse-slow",
		capabilities: ["color", "animation"],
		animations: BADGE_ANIMATIONS,
		preview: "badge",
	},
	{
		id: "pr.merged",
		group: "prBadge",
		label: "Merged",
		description: "PR merged",
		colorVar: "--ind-pr-merged",
		defaultColor: "var(--merged)",
		capabilities: ["color"],
		preview: "badge",
	},
	{
		id: "pr.closed",
		group: "prBadge",
		label: "Closed",
		description: "PR closed without merging",
		colorVar: "--ind-pr-closed",
		defaultColor: "var(--error)",
		capabilities: ["color"],
		preview: "badge",
	},

	// -------------------------------------------------------------------
	// Git Repo Status — src/components/Sidebar/RepoSection.tsx's per-worktree
	// operation badge (rebasing/merging/cherry-picking/reverting/bisecting)
	// and src/components/GitPanel/ChangesTab.tsx's conflicts banner.
	//
	// Deliberately NOT included here: `detached`/`ahead`/`behind`/`diverged`/
	// `stashes`. Nothing in the running app renders those yet — their natural
	// home, GitPanel/SyncRow.tsx, is dead code (never mounted; see the
	// customization plan's Phase 5 "Known follow-up" note) and wiring it up
	// is explicitly out of scope for this phase. Phase 1 of this same plan
	// deleted the legend's old "Panels" section for exactly this reason
	// ("documenting behavior that does not exist is worse than omitting
	// it") — add these once something actually applies them.
	// -------------------------------------------------------------------
	// Color + animation only — RepoSection.tsx renders these as a colored text pill
	// (matching PrStateBadge.tsx's own convention), never an icon shape, so an
	// icon capability here would be exactly the "customization the app doesn't
	// apply" mistake Phase 1's Panels-section removal was about.
	{
		id: "gitState.rebasing",
		group: "gitState",
		label: "Rebasing",
		description: "Rebase in progress",
		colorVar: "--ind-gitstate-rebasing",
		defaultColor: "var(--attention)",
		animVar: "--ind-anim-gitstate-rebasing",
		defaultAnimation: "pulse-slow",
		capabilities: ["color", "animation"],
		animations: BADGE_ANIMATIONS,
		preview: "badge",
	},
	{
		id: "gitState.merging",
		group: "gitState",
		label: "Merging",
		description: "Merge in progress",
		colorVar: "--ind-gitstate-merging",
		defaultColor: "var(--merged)",
		animVar: "--ind-anim-gitstate-merging",
		defaultAnimation: "pulse-slow",
		capabilities: ["color", "animation"],
		animations: BADGE_ANIMATIONS,
		preview: "badge",
	},
	{
		id: "gitState.cherryPicking",
		group: "gitState",
		label: "Cherry-picking",
		description: "Cherry-pick in progress",
		colorVar: "--ind-gitstate-cherry-picking",
		defaultColor: "var(--changes)",
		animVar: "--ind-anim-gitstate-cherry-picking",
		defaultAnimation: "pulse-slow",
		capabilities: ["color", "animation"],
		animations: BADGE_ANIMATIONS,
		preview: "badge",
	},
	{
		id: "gitState.reverting",
		group: "gitState",
		label: "Reverting",
		description: "Revert in progress",
		colorVar: "--ind-gitstate-reverting",
		defaultColor: "var(--warning)",
		animVar: "--ind-anim-gitstate-reverting",
		defaultAnimation: "pulse-slow",
		capabilities: ["color", "animation"],
		animations: BADGE_ANIMATIONS,
		preview: "badge",
	},
	{
		id: "gitState.bisecting",
		group: "gitState",
		label: "Bisecting",
		description: "Bisect in progress",
		colorVar: "--ind-gitstate-bisecting",
		defaultColor: "var(--unseen)",
		animVar: "--ind-anim-gitstate-bisecting",
		defaultAnimation: "pulse-slow",
		capabilities: ["color", "animation"],
		animations: BADGE_ANIMATIONS,
		preview: "badge",
	},
	// The one gitState entry that DOES render an icon — ChangesTab.tsx's conflicts
	// banner uses IndicatorIcon, unlike the plain text pills above.
	{
		id: "gitState.conflicts",
		group: "gitState",
		label: "Conflicts",
		description: "Unmerged files in the working tree",
		colorVar: "--ind-gitstate-conflicts",
		defaultColor: "var(--error)",
		animVar: "--ind-anim-gitstate-conflicts",
		defaultAnimation: "pulse",
		defaultIconId: "bang",
		capabilities: ["color", "icon", "animation"],
		animations: BADGE_ANIMATIONS,
		preview: "badge",
	},

	// -------------------------------------------------------------------
	// Diff Stats — src/components/Sidebar/RepoSection.tsx StatsBadge.
	// -------------------------------------------------------------------
	{
		id: "diffStat.additions",
		group: "diffStat",
		label: "Additions",
		description: "Lines added vs main",
		colorVar: "--ind-diffstat-additions",
		defaultColor: "var(--success)",
		capabilities: ["color"],
		preview: "symbol",
	},
	{
		id: "diffStat.deletions",
		group: "diffStat",
		label: "Deletions",
		description: "Lines removed vs main",
		colorVar: "--ind-diffstat-deletions",
		defaultColor: "var(--error)",
		capabilities: ["color"],
		preview: "symbol",
	},
];

export function getIndicator(id: string): IndicatorDef | undefined {
	return INDICATORS.find((entry) => entry.id === id);
}

export function indicatorsByGroup(group: IndicatorGroup): IndicatorDef[] {
	return INDICATORS.filter((entry) => entry.group === group);
}

/**
 * The icon a registry entry should render right now: the user's override if one is set and
 * valid, else the entry's own default, else the generic "dot" fallback. The single source of
 * truth for this lookup — every real icon-rendering site (terminal tab dot, sidebar branch
 * icon, conflicts banner, and the UI Legend's own preview/editor) must go through this, not
 * reimplement the override lookup locally, so a fix or a validation tightening lands everywhere
 * at once instead of needing to be copied to each call site by hand.
 */
export function resolveIconId(overrides: readonly IndicatorOverride[], id: string): IconId {
	const overrideIcon = overrides.find((o) => o.id === id)?.icon;
	if (overrideIcon && isKnownIconId(overrideIcon)) return overrideIcon;
	return getIndicator(id)?.defaultIconId ?? "dot";
}

/** Same idea as `resolveIconId`, for animation. Colors don't need an equivalent — they resolve
 *  automatically wherever CSS reads `var(--ind-*)`, but a picker/dialog that needs to know the
 *  current animation id (to preselect it) can't get that from a CSS variable alone. */
export function resolveAnimationId(overrides: readonly IndicatorOverride[], id: string): AnimationId {
	const overrideAnimation = overrides.find((o) => o.id === id)?.animation;
	if (overrideAnimation && isKnownAnimationId(overrideAnimation)) return overrideAnimation;
	return getIndicator(id)?.defaultAnimation ?? "none";
}

export const GROUP_LABELS: Record<IndicatorGroup, string> = {
	terminalStatus: "Terminal Status Dots",
	tabType: "Tab Types",
	sidebarSymbol: "Sidebar Symbols",
	prBadge: "PR Status Badges",
	gitState: "Git Repo Status",
	diffStat: "Diff Stats",
};

export const GROUP_HINTS: Partial<Record<IndicatorGroup, string>> = {
	terminalStatus: "The colored dot on each terminal tab",
	tabType: "Background tint and bottom border color by tab type",
	prBadge: "Shown next to branches with a pull request",
	gitState: "In-progress git operations and unmerged conflicts",
};
