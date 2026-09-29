import { type Component, createSignal, For, type JSX, Show } from "solid-js";
import { t } from "../../i18n";
import { IndicatorEditorDialog } from "../../indicators/IndicatorEditorDialog";
import { IndicatorPreview } from "../../indicators/IndicatorPreview";
import {
	GROUP_HINTS,
	GROUP_LABELS,
	getIndicator,
	type IndicatorDef,
	type IndicatorGroup,
	indicatorsByGroup,
} from "../../indicators/registry";
import { settingsStore } from "../../stores/settings";
import { SettingToggle } from "../SettingsPanel/SettingFields";
import { PrStateBadge } from "../Sidebar/PrStateBadge";
import { BranchIcon, type BranchIconProps, UnmergedMarker } from "../Sidebar/RepoSection";
import s from "./UiLegend.module.css";

// ---------------------------------------------------------------------------
// Legend data
//
// Rows that explain a sidebar/toolbar marker render the REAL component (so the
// legend cannot drift from what a branch row shows). Rows that are pure
// swatches (terminal dots, tab types, git repo status, diff stats) render from
// `src/indicators/registry.ts`. Either kind may carry an `indicatorId`: in
// editable mode (Settings > Appearance) clicking that row's preview opens the
// entry's combined color/icon/animation editor, and the override flows into
// the real component the row renders.
// ---------------------------------------------------------------------------

/** Sidebar row icons. Each entry renders the real `BranchIcon` with the props
 *  that select its shape, so the legend can never drift from the sidebar. */
export interface BranchIconEntry {
	icon: BranchIconProps;
	label: string;
	description: string;
	/** Registry entry this row edits. Absent for states whose color belongs to
	 *  a terminal status dot (Awaiting input / Error) — edit those there. */
	indicatorId?: string;
}

export const SIDEBAR_SYMBOL_LEGEND: BranchIconEntry[] = [
	{
		icon: { isMainBranch: true, isMainWorktree: true, branchHasTerminals: true },
		label: "Main branch",
		description: "Primary branch (main/master)",
		indicatorId: "sidebar.main",
	},
	{
		icon: { isMainBranch: false, isMainWorktree: true, branchHasTerminals: true },
		label: "Feature branch",
		description: "Main worktree switched to another branch",
		indicatorId: "sidebar.branch",
	},
	{
		icon: { isMainBranch: false, isMainWorktree: false, branchHasTerminals: true },
		label: "Worktree",
		description: "Linked git worktree",
		indicatorId: "sidebar.worktree",
	},
	{
		icon: { isMainBranch: false, isMainWorktree: false, isShell: true, branchHasTerminals: true },
		label: "Shell",
		description: "Folder without a git repository",
		indicatorId: "sidebar.shell",
	},
	{
		icon: { isMainBranch: false, isMainWorktree: false, hasQuestion: true, branchHasTerminals: true },
		label: "Awaiting input",
		description: "A terminal needs input",
	},
	{
		icon: { isMainBranch: false, isMainWorktree: false, hasError: true, branchHasTerminals: true },
		label: "Error",
		description: "API error or agent stuck",
	},
	{
		icon: { isMainBranch: false, isMainWorktree: false, branchHasTerminals: false },
		label: "Idle",
		description: "No open terminal on this row",
		indicatorId: "sidebar.idle",
	},
];

/** Rendered with the real sidebar component, so the legend cannot drift from
 *  what a branch row shows — color, shape and pulse included. */
export interface PrLegendEntry {
	label: string;
	description: string;
	badge: Partial<Parameters<typeof PrStateBadge>[0]>;
	/** Registry entry (`pr.<prBadgeKind>`) whose override recolors this marker. */
	indicatorId: string;
}

export const PR_BADGE_LEGEND: PrLegendEntry[] = [
	{ label: "Open", description: "Open PR", badge: { state: "open" }, indicatorId: "pr.open" },
	{
		label: "Ready",
		description: "Approved and mergeable",
		badge: { state: "open", mergeable: "MERGEABLE", reviewDecision: "APPROVED" },
		indicatorId: "pr.ready",
	},
	{ label: "Draft", description: "PR is a draft", badge: { state: "open", isDraft: true }, indicatorId: "pr.draft" },
	{
		label: "Conflicts",
		description: "Merge conflicts (pulsing diamond)",
		badge: { state: "open", conflictState: "conflicting" },
		indicatorId: "pr.conflict",
	},
	{
		label: "Checking",
		description: "GitHub is recomputing mergeability (pulsing)",
		badge: { state: "open", conflictState: "checking" },
		indicatorId: "pr.checking",
	},
	{
		label: "CI Failed",
		description: "CI checks failed",
		badge: { state: "open", ciFailed: 1 },
		indicatorId: "pr.ci-failed",
	},
	{
		label: "Changes Req.",
		description: "Changes requested",
		badge: { state: "open", reviewDecision: "CHANGES_REQUESTED" },
		indicatorId: "pr.changes-requested",
	},
	{
		label: "Comments",
		description: "Unresolved review threads (bot and human)",
		badge: { state: "open", unresolvedThreads: 1 },
		indicatorId: "pr.unresolved-comments",
	},
	{
		label: "Review",
		description: "Awaiting review",
		badge: { state: "open", reviewDecision: "REVIEW_REQUIRED" },
		indicatorId: "pr.review-required",
	},
	{
		label: "CI Running",
		description: "CI in progress (pulsing)",
		badge: { state: "open", ciPending: 1 },
		indicatorId: "pr.ci-pending",
	},
	{ label: "Merged", description: "PR merged", badge: { state: "merged" }, indicatorId: "pr.merged" },
	{ label: "Closed", description: "PR closed without merging", badge: { state: "closed" }, indicatorId: "pr.closed" },
];

interface SymbolEntry {
	symbol: string;
	label: string;
	description: string;
}

const TOOLBAR_COUNT_LEGEND: SymbolEntry[] = [
	{
		symbol: "↑N",
		label: "Ahead",
		description: "Selected branch has N commits not pushed to its upstream. Absent without an upstream",
	},
	{ symbol: "↓N", label: "Behind", description: "Selected branch is N commits behind its upstream" },
];

/** Registry entries already edited through a real-component row above; the
 *  rest of their group is listed after those rows so no entry is left
 *  without an editor. */
const SIDEBAR_ROW_IDS = new Set(SIDEBAR_SYMBOL_LEGEND.map((e) => e.indicatorId).filter(Boolean));
const PR_ROW_IDS = new Set(PR_BADGE_LEGEND.map((e) => e.indicatorId));

/** Binds a group's visibility toggle to its settingsStore field. Groups not
 *  listed here (terminalStatus, sidebarSymbol) have no show/hide setting —
 *  those indicators aren't optional the way a whole badge/tint/section is. */
function groupToggleBinding(
	group: IndicatorGroup,
): { checked: boolean; onChange: (v: boolean) => void; label: string } | undefined {
	switch (group) {
		case "tabType":
			return {
				checked: settingsStore.state.tabTypeHighlighting,
				onChange: (v) => settingsStore.setTabTypeHighlighting(v),
				label: t("uiLegend.toggle.tabTypeHighlighting", "Show tab type highlighting"),
			};
		case "prBadge":
			return {
				checked: settingsStore.state.showPrBadges,
				onChange: (v) => settingsStore.setShowPrBadges(v),
				label: t("uiLegend.toggle.showPrBadges", "Show PR status badges"),
			};
		case "gitState":
			return {
				checked: settingsStore.state.showGitState,
				onChange: (v) => settingsStore.setShowGitState(v),
				label: t("uiLegend.toggle.showGitState", "Show git repo status indicators"),
			};
		case "diffStat":
			return {
				checked: settingsStore.state.showDiffStats,
				onChange: (v) => settingsStore.setShowDiffStats(v),
				label: t("uiLegend.toggle.showDiffStats", "Show diff stats"),
			};
		default:
			return undefined;
	}
}

/**
 * Visual reference for every color, icon, and animation used throughout the
 * app. Marker rows render the real sidebar components (BranchIcon,
 * UnmergedMarker, compact PrStateBadge); swatch rows render from
 * `src/indicators/registry.ts`, the single source of truth for every
 * customizable indicator.
 *
 * `editable` turns each row backed by a registry entry into an editor: the
 * row's own preview (the real BranchIcon / PR marker, or the registry swatch)
 * becomes a button opening `IndicatorEditorDialog` — one combined dialog
 * holding that entry's color, icon, and animation controls, whichever it has
 * — plus a reset "×" that clears the whole override; it also adds each
 * group's show/hide toggle. Used by Settings → Appearance. `HelpPanel.tsx`'s
 * reference view stays read-only: the preview there is inert, not a button.
 */
export const UiLegend: Component<{ editable?: boolean }> = (props) => {
	const [editingId, setEditingId] = createSignal<string | null>(null);

	const overrideFor = (id: string) => settingsStore.state.indicatorOverrides.find((o) => o.id === id);

	/** Any field set at all — not just color — so the reset "×" also shows
	 *  for an icon-only or animation-only override. */
	const hasOverride = (id: string): boolean => {
		const o = overrideFor(id);
		return !!o && (o.color !== undefined || o.icon !== undefined || o.animation !== undefined);
	};

	/** A row's preview. In editable mode, a row backed by a registry entry
	 *  turns it into the button that opens that entry's editor dialog. */
	const EditablePreview: Component<{ id?: string; children: JSX.Element }> = (p) => (
		<Show when={props.editable && p.id && getIndicator(p.id) ? p.id : undefined} fallback={p.children}>
			{(id) => (
				<button
					class={s.previewBtn}
					onClick={() => setEditingId(id())}
					title={t("uiLegend.btn.customize", "Customize")}
				>
					{p.children}
				</button>
			)}
		</Show>
	);

	/** Reset "×" for a row whose entry has an override; editable mode only. */
	const ResetButton: Component<{ id?: string }> = (p) => (
		<Show when={props.editable && p.id && hasOverride(p.id) ? p.id : undefined}>
			{(id) => (
				<button
					class={s.resetSwatch}
					onClick={() => settingsStore.clearIndicatorOverride(id())}
					title={t("uiLegend.btn.resetOverride", "Reset to default")}
				>
					&times;
				</button>
			)}
		</Show>
	);

	/** A legend section: heading, optional hint, the group's show/hide toggle
	 *  (editable mode only), then its rows. */
	const Group: Component<{ label: string; hint?: string; toggleGroup?: IndicatorGroup; children: JSX.Element }> = (
		p,
	) => (
		<div class={s.group}>
			<label class={s.groupLabel}>{p.label}</label>
			<Show when={p.hint}>
				<p class={s.hint}>{p.hint}</p>
			</Show>
			<Show when={props.editable && p.toggleGroup ? groupToggleBinding(p.toggleGroup) : undefined}>
				{(toggle) => <SettingToggle checked={toggle().checked} onChange={toggle().onChange} label={toggle().label} />}
			</Show>
			<div class={s.grid}>{p.children}</div>
		</div>
	);

	const RegistryRow: Component<{ entry: IndicatorDef }> = (p) => (
		<div class={s.row}>
			<EditablePreview id={p.entry.id}>
				<IndicatorPreview entry={p.entry} />
			</EditablePreview>
			<span class={s.label}>{p.entry.label}</span>
			<span class={s.desc}>{p.entry.description}</span>
			<ResetButton id={p.entry.id} />
		</div>
	);

	const registryGroup = (group: IndicatorGroup, skip?: Set<string | undefined>) => (
		<For each={indicatorsByGroup(group).filter((e) => !skip?.has(e.id))}>
			{(entry) => <RegistryRow entry={entry} />}
		</For>
	);

	return (
		<div class={s.legend}>
			<Group label={GROUP_LABELS.terminalStatus} hint={GROUP_HINTS.terminalStatus}>
				{registryGroup("terminalStatus")}
			</Group>

			<Group label={GROUP_LABELS.tabType} hint={GROUP_HINTS.tabType} toggleGroup="tabType">
				{registryGroup("tabType")}
			</Group>

			<Group label={GROUP_LABELS.tabActivity} hint={GROUP_HINTS.tabActivity}>
				{registryGroup("tabActivity")}
			</Group>

			{/* Sidebar branch icons — the real BranchIcon, then the registry's
			    remaining sidebar entries (merged / remote badges) */}
			<Group label={GROUP_LABELS.sidebarSymbol} hint="The icon at the start of each sidebar row">
				<For each={SIDEBAR_SYMBOL_LEGEND}>
					{(entry) => (
						<div class={s.row}>
							<EditablePreview id={entry.indicatorId}>
								<span class={s.symbol}>
									<BranchIcon {...entry.icon} />
								</span>
							</EditablePreview>
							<span class={s.label}>{entry.label}</span>
							<span class={s.desc}>{entry.description}</span>
							<ResetButton id={entry.indicatorId} />
						</div>
					)}
				</For>
				{registryGroup("sidebarSymbol", SIDEBAR_ROW_IDS)}
			</Group>

			{/* Unmerged marker */}
			<Group label="Branch Markers" hint="Shown at the end of a sidebar branch row">
				<div class={s.row}>
					<span class={s.symbol}>
						<UnmergedMarker />
					</span>
					<span class={s.label}>Unmerged</span>
					<span class={s.desc}>Commits not merged into the default branch. Not a dirty worktree</span>
				</div>
			</Group>

			{/* PR badges — the real compact marker; its color/animation come from
			    the row's registry entry, so an override shows here and on every
			    branch row alike */}
			<Group
				label={GROUP_LABELS.prBadge}
				hint="Shown next to branches with a pull request. Hover the marker for the state name"
				toggleGroup="prBadge"
			>
				<For each={PR_BADGE_LEGEND}>
					{(entry) => (
						<div class={s.row}>
							<EditablePreview id={entry.indicatorId}>
								<PrStateBadge compact prNumber={42} {...entry.badge} />
							</EditablePreview>
							<span class={s.label}>{entry.label}</span>
							<span class={s.desc}>{entry.description}</span>
							<ResetButton id={entry.indicatorId} />
						</div>
					)}
				</For>
				{registryGroup("prBadge", PR_ROW_IDS)}
			</Group>

			<Group label={GROUP_LABELS.gitState} hint={GROUP_HINTS.gitState} toggleGroup="gitState">
				{registryGroup("gitState")}
			</Group>

			{/* Toolbar ahead/behind */}
			<Group label="Toolbar Branch Counts" hint="Next to the branch name in the toolbar">
				<For each={TOOLBAR_COUNT_LEGEND}>
					{(entry) => (
						<div class={s.row}>
							<span class={s.symbol}>{entry.symbol}</span>
							<span class={s.label}>{entry.label}</span>
							<span class={s.desc}>{entry.description}</span>
						</div>
					)}
				</For>
			</Group>

			<Group label={GROUP_LABELS.diffStat} hint={GROUP_HINTS.diffStat} toggleGroup="diffStat">
				{registryGroup("diffStat")}
			</Group>

			<Show when={props.editable}>
				<button class={s.resetAllBtn} onClick={() => settingsStore.resetAllIndicators()}>
					{t("uiLegend.btn.resetAll", "Reset all indicators")}
				</button>
				<IndicatorEditorDialog indicatorId={editingId()} onClose={() => setEditingId(null)} />
			</Show>
		</div>
	);
};
