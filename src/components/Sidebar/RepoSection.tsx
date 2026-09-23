import { type Component, createMemo, createSignal, For, Show } from "solid-js";
import { shortenHomePath } from "../../platform";
import { appLogger } from "../../stores/appLogger";
import { githubStore } from "../../stores/github";
import type { RepositoryState, WorkspaceState } from "../../stores/repositories";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";
import { writeClipboard } from "../../utils/clipboard";
import { _resetMergedActivityAccum, activePrStatus } from "../../utils/mergedPrGrace";
import { effectiveMergeMethod } from "../../utils/prMerge";

export { effectiveMergeMethod };

import { t } from "../../i18n";
import { invoke } from "../../invoke";
import { contextMenuActionsStore } from "../../stores/contextMenuActionsStore";
import { rateLimitStore } from "../../stores/ratelimit";
import { remoteConnectionsStore } from "../../stores/remoteConnections";
import { repoSettingsStore } from "../../stores/repoSettings";
import { settingsStore } from "../../stores/settings";
import { sidebarPluginStore } from "../../stores/sidebarPluginStore";
import { cx } from "../../utils";
import { onClickKeyDown } from "../../utils/a11y";
import { displayTask, effectiveActivityState } from "../../utils/activitySnapshot";
import { compareBranches } from "../../utils/branchSort";
import { keyFor } from "../../utils/hotkey";
import { navigateToTerminal } from "../../utils/navigateToTerminal";
import { handleOpenUrl } from "../../utils/openUrl";
import { timeSync } from "../../utils/perfTrace";
import type { ContextMenuItem } from "../ContextMenu";
import { ContextMenu, createContextMenu } from "../ContextMenu";
import { remoteUrlToGitHub } from "../GitPanel/BranchesTab";
import { PromptDialog } from "../PromptDialog";
import b from "../shared/branch.module.css";
import { AgentIcon } from "../ui/AgentIcon";
import { ChevronIcon } from "../ui/ChevronIcon";
import { PrStateBadge } from "./PrStateBadge";
import s from "./Sidebar.module.css";
import { SidebarPluginSection } from "./SidebarPluginSection";

const BRANCH_ICON_CLASSES: Record<string, string> = {
	main: s.branchIconMain,
	worktree: s.branchIconWorktree,
	error: s.branchIconError,
	question: s.branchIconQuestion,
	activity: s.branchIconActivity,
	unseen: s.branchIconUnseen,
	idle: s.branchIconIdle,
};

/** Branch icon component — icon shape and color driven by terminal state.
 *
 *  Icon shapes:
 *  - Main worktree + main branch → star
 *  - Main worktree + non-main branch (after switch) → branch icon
 *  - Linked worktree → worktree fork icon
 *  - Shell (non-git dir) → terminal icon
 *  - Question (awaiting input) → "?" (overrides all)
 *
 *  Color priority (highest wins):
 *  1. question  → --attention (pulsing)
 *  2. busy      → --activity  (pulsing)
 *  3. unseen    → --unseen    (static purple)
 *  4. idle      → --fg-muted  (no open terminal on this branch)
 *  5. base      → --warning (main) or --success (worktree)
 */
export const BranchIcon: Component<{
	isMainBranch: boolean;
	isMainWorktree: boolean;
	isShell?: boolean;
	hasError?: boolean;
	hasQuestion?: boolean;
	hasBusy?: boolean;
	hasUnseen?: boolean;
	branchHasTerminals?: boolean;
}> = (props) => {
	const iconShape = () => {
		if (props.hasError) return "error";
		if (props.hasQuestion) return "question";
		if (props.isShell) return "shell";
		if (props.isMainWorktree && props.isMainBranch) return "star";
		if (props.isMainWorktree) return "branch";
		return "worktree";
	};

	/** Single source of truth for icon color — priority cascade.
	 *  Error > question > busy > unseen > idle > base.
	 *  A branch with no open terminal is idle (grey), even when other branches
	 *  in the same repo have tabs open — the base color (yellow for main, green
	 *  for worktree) means "has an open tab here, nothing special happening". */
	const colorClass = () => {
		if (props.hasError) return "error";
		if (props.hasQuestion) return "question";
		if (props.hasBusy) return "activity";
		if (props.hasUnseen) return "unseen";
		if (props.branchHasTerminals === false) return "idle";
		if (props.isMainBranch) return "main";
		return "worktree";
	};

	return (
		<span class={cx(s.branchIcon, BRANCH_ICON_CLASSES[colorClass()])}>
			{(() => {
				switch (iconShape()) {
					case "error":
						return "!";
					case "question":
						return "?";
					case "shell":
						return (
							<svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor">
								<path d="M1 3l5 5-5 5h2l5-5-5-5H1zm7 9h7v2H8v-2z" />
							</svg>
						);
					case "star":
						return (
							<svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor">
								<path d="M9.2 1.2v4.4L13 3.2a1.3 1.3 0 1 1 1.3 2.3L10.5 8l3.8 2.5a1.3 1.3 0 1 1-1.3 2.3L9.2 10.4v4.4a1.2 1.2 0 0 1-2.4 0v-4.4L3 13a1.3 1.3 0 1 1-1.3-2.3L5.5 8 1.7 5.5A1.3 1.3 0 0 1 3 3.2l3.8 2.4V1.2a1.2 1.2 0 0 1 2.4 0z" />
							</svg>
						);
					case "worktree":
						return (
							<svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor">
								<path
									d="M5 1.5a1.5 1.5 0 1 1 0 3 1.5 1.5 0 0 1 0-3zm0 10a1.5 1.5 0 1 1 0 3 1.5 1.5 0 0 1 0-3zm6-4a1.5 1.5 0 1 1 0 3 1.5 1.5 0 0 1 0-3zM5 5v2.5a2 2 0 0 0 2 2h2.5M5 10.5V8"
									fill="none"
									stroke="currentColor"
									stroke-width="1.5"
									stroke-linecap="round"
								/>
							</svg>
						);
					default:
						return (
							<svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor">
								<path d="M11.75 2.5a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5zm-2.25.75a2.25 2.25 0 1 1 3 2.122V6A2.5 2.5 0 0 1 10 8.5H6a1 1 0 0 0-1 1v1.128a2.251 2.251 0 1 1-1.5 0V5.372a2.25 2.25 0 1 1 1.5 0v1.836A2.493 2.493 0 0 1 6 7h4a1 1 0 0 0 1-1v-.628A2.25 2.25 0 0 1 9.5 3.25zM4.25 12a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5zM3.5 3.25a.75.75 0 1 1 1.5 0 .75.75 0 0 1-1.5 0z" />
							</svg>
						);
				}
			})()}
		</span>
	);
};

/** Stats badge component - shows additions/deletions */
function compactStat(value: number): string {
	if (value < 1000) return String(value);
	const compact = (value / 1000).toFixed(value < 10_000 ? 1 : 0);
	return `${compact.replace(/\.0$/, "")}k`;
}

export const StatsBadge: Component<{
	additions: number;
	deletions: number;
	/** Files a removal would discard. Carried in the tooltip rather than a chip
	 *  of its own: a second chip on the same row said the same thing twice. */
	dirtyFiles?: number;
	onClick?: (e: MouseEvent | KeyboardEvent) => void;
}> = (props) => (
	<Show when={props.additions > 0 || props.deletions > 0}>
		<div
			class={s.branchStats}
			data-tooltip={`Tracked line changes: +${props.additions} -${props.deletions}${
				props.dirtyFiles ? ` — ${props.dirtyFiles} uncommitted file${props.dirtyFiles === 1 ? "" : "s"}` : ""
			}`}
			data-tooltip-pos="bottom"
			data-tooltip-align="right"
			role={props.onClick ? "button" : undefined}
			tabIndex={props.onClick ? 0 : undefined}
			onClick={props.onClick}
			onKeyDown={props.onClick ? onClickKeyDown((e) => props.onClick!(e)) : undefined}
			style={props.onClick ? { cursor: "pointer" } : undefined}
		>
			<span class={s.statAdd}>+{compactStat(props.additions)}</span>
			<span class={s.statDel}>-{compactStat(props.deletions)}</span>
		</div>
	</Show>
);

export { _resetMergedActivityAccum };

/**
 * Whether a branch shows its nested terminal-tab list. Single source of truth for
 * the feature: gated by the `tabTreeEnabled` setting and available whenever the branch
 * has a terminal. When off, the aria state, row-click toggle and
 * the list itself are all inert.
 */
function getBranchTabsAvailable(branch: WorkspaceState): boolean {
	return settingsStore.state.tabTreeEnabled && branch.terminals.length > 0;
}

function compactActivityAge(timestamp: number | null): string {
	if (!timestamp) return "";
	const minutes = Math.max(0, Math.floor((Date.now() - timestamp) / 60_000));
	if (minutes < 1) return "<1m";
	if (minutes < 60) return `${minutes}m`;
	const hours = Math.floor(minutes / 60);
	if (hours < 24) return `${hours}h`;
	return `${Math.floor(hours / 24)}d`;
}

/** Collapsible activity card for the terminals attached to a branch. */
const BranchTabList: Component<{ terminalIds: string[] }> = (props) => {
	return (
		<div class={s.branchTabList} role="group" aria-label="Terminal tabs">
			<For each={props.terminalIds}>
				{(id) => {
					const term = () => terminalsStore.get(id);
					const isActive = () => terminalsStore.state.activeId === id;
					// The row shows the tab title, as the tab bar does; what the agent is
					// doing goes in the tooltip.
					const activity = () => term()?.name ?? null;
					const detail = () => {
						const t = term();
						return t ? (t.agentIntent ?? displayTask(t.currentTask, t.agentType) ?? t.lastPrompt) : null;
					};
					const accessibleLabel = () => {
						const t = term();
						if (!t) return undefined;
						const d = detail();
						return d ? `${t.name}: ${d}` : t.name;
					};
					const dotClass = () => {
						const t = term();
						if (!t) return s.branchTabDot;
						const state = effectiveActivityState(
							t.shellState,
							t.awaitingInput,
							!!(t.sessionId && rateLimitStore.isRateLimited(t.sessionId)),
							t.agentState,
							t.backgroundWork,
						);
						if (state === "error") return cx(s.branchTabDot, s.branchTabDotError);
						if (state === "awaiting_input") return cx(s.branchTabDot, s.branchTabDotQuestion);
						if (state === "working" || state === "rate_limited") return cx(s.branchTabDot, s.branchTabDotBusy);
						if (t.unseen) return cx(s.branchTabDot, s.branchTabDotUnseen);
						if (state === "idle" || state === "completed") return cx(s.branchTabDot, s.branchTabDotIdle);
						return s.branchTabDot;
					};

					return (
						<Show when={term()}>
							{(t) => (
								<button
									class={cx(s.branchTabItem, isActive() && s.active)}
									onClick={() => navigateToTerminal(id)}
									title={accessibleLabel()}
									aria-label={accessibleLabel()}
								>
									<span class={dotClass()} aria-hidden="true" />
									<Show
										when={t().agentType}
										fallback={
											<span class={s.branchAgentIcon} aria-hidden="true">
												<svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor">
													<path d="M1 3l5 5-5 5h2l5-5-5-5H1zm7 9h7v2H8v-2z" />
												</svg>
											</span>
										}
									>
										{(agent) => (
											<span class={s.branchAgentIcon} aria-hidden="true">
												<AgentIcon agent={agent()} size={14} />
											</span>
										)}
									</Show>
									<Show when={activity()}>{(label) => <span class={s.branchAgentActivity}>{label()}</span>}</Show>
									<Show when={terminalsStore.getSubAgentTag(id)}>
										{(parent) => (
											<span class={s.branchSubAgentTag} title={`Spawned by ${parent()}`}>
												↳ {parent()}
											</span>
										)}
									</Show>
									<span class={s.branchAgentTime}>{compactActivityAge(t().lastDataAt)}</span>
								</button>
							)}
						</Show>
					);
				}}
			</For>
		</div>
	);
};

/** Branch item component */
export const BranchItem: Component<{
	branch: WorkspaceState;
	repoPath: string;
	isActive: boolean;
	canRemove: boolean;
	shortcutIndex?: number;
	agentMenuItems?: () => ContextMenuItem[];
	onSelect: () => void;
	onAddTerminal: () => void;
	isRemoving?: boolean;
	onRemove: () => void;
	onRename: () => void;
	onCreateBranch?: () => void;
	onShowPrDetail: () => void;
	onShowChanges?: () => void;
	onCreateWorktreeFromBranch?: () => void;
	onMergeAndArchive?: () => void;
	onSwitchBranch?: (branchName: string) => void;
	switchBranchList?: () => string[];
	currentBranch?: () => string;
	githubBaseUrl?: string | null;
	onSetLabel?: (currentLabel: string | undefined) => void;
	/** Set only when another row of this repo is on the same branch, in which case
	 *  the branch name alone does not identify the row. Holds the workspace's
	 *  directory leaf — the one thing that differs. */
}> = (props) => {
	const ctxMenu = createContextMenu();

	const branchLabel = createMemo(
		() => repoSettingsStore.getEffectiveField(props.repoPath, "branchLabels")?.[props.branch.branchName],
	);

	/** Hover text: enough to tell two same-branch rows apart without widening the row. */
	const rowTitle = createMemo(() => {
		const parts = [branchLabel(), props.branch.branchName].filter(Boolean);
		return parts.join(" — ");
	});

	const pr = createMemo(() => activePrStatus(props.repoPath, props.branch.branchName));
	const checks = createMemo(() => githubStore.getCheckSummary(props.repoPath, props.branch.branchName));
	const hasDiff = () => props.branch.additions > 0 || props.branch.deletions > 0;
	// Select this branch/worktree first so the Git panel targets it (it follows
	// activeWorktreePath), then open the changes tab — otherwise a chip would
	// show the active branch's diff instead of this row's.
	const showChanges = (e: MouseEvent | KeyboardEvent) => {
		e.stopPropagation();
		props.onSelect();
		props.onShowChanges?.();
	};

	const hasError = () => props.branch.terminals.some((id) => terminalsStore.get(id)?.awaitingInput === "error");

	const hasQuestion = () => props.branch.terminals.some((id) => terminalsStore.get(id)?.awaitingInput === "question");

	// Debounced busy — centralized in terminalsStore with 2s hold
	const hasBusy = () => props.branch.terminals.some((id) => terminalsStore.isBusy(id));

	const hasUnseen = () => props.branch.terminals.some((id) => terminalsStore.get(id)?.unseen);

	const handleDoubleClick = (e: MouseEvent) => {
		e.stopPropagation();
		if (props.branch.isMain || props.branch.isShell) {
			if (props.branch.savedTerminals?.length) return;
			props.onAddTerminal();
		} else {
			props.onRename();
		}
	};

	// A row click opens the branch and nothing else (AGENTS.md "Sidebar clicks").
	// Only the branch icon expands or collapses the agents, and it stops
	// propagation so the toggle never also opens the branch.
	const toggleAgents = (e: MouseEvent | KeyboardEvent) => {
		e.stopPropagation();
		repositoriesStore.toggleWorkspaceTabsCollapsed(props.repoPath, props.branch.workspaceId);
	};

	const handleCopyPath = async () => {
		const path = props.branch.worktreePath;
		if (path) {
			try {
				await writeClipboard(shortenHomePath(path));
			} catch (err) {
				appLogger.warn("app", "Failed to copy path to clipboard", err);
			}
		}
	};

	const contextMenuItems = (): ContextMenuItem[] => {
		const isShell = props.branch.isShell;
		const hasBranch = !isShell && !!props.branch.branchName;
		const isLinkedWorktree = !!props.branch.worktreePath && props.branch.worktreePath !== props.repoPath;
		const isMainWorktree = props.branch.worktreePath === props.repoPath;

		// Group 1 — quick actions, ordered by real usage frequency (Copy Path and the
		// GitHub links are the most-used, so they lead).
		const quick: ContextMenuItem[] = [
			{
				label: "Copy Path",
				title: props.branch.worktreePath ? shortenHomePath(props.branch.worktreePath) : undefined,
				action: handleCopyPath,
				disabled: !props.branch.worktreePath,
			},
		];
		if (hasBranch && props.githubBaseUrl) {
			const ghBase = props.githubBaseUrl;
			const branchUrl = `${ghBase}/tree/${encodeURIComponent(props.branch.branchName)}`;
			quick.push({ label: "Open in GitHub", action: () => handleOpenUrl(branchUrl) });
			const prStatus = githubStore.getPrStatus(props.repoPath, props.branch.branchName);
			if (prStatus?.url) {
				quick.push({ label: "Open PR", action: () => handleOpenUrl(prStatus.url) });
			}
		}
		quick.push({ label: "Add Terminal", action: props.onAddTerminal });
		if (!isShell) {
			const agentItems = props.agentMenuItems?.();
			if (agentItems && agentItems.length > 0) quick.push(...agentItems);
		}
		// "Switch Branch" submenu — only on the main worktree row.
		if (isMainWorktree && props.onSwitchBranch && props.switchBranchList && props.currentBranch) {
			const switchBranch = props.onSwitchBranch;
			const current = props.currentBranch();
			const branchList = props.switchBranchList();
			if (branchList.length > 0) {
				const branchChildren: ContextMenuItem[] = branchList.map((name) => ({
					label: name === current ? `${name}  \u2713` : name,
					action: () => {
						if (name !== current) switchBranch(name);
					},
					disabled: name === current,
				}));
				quick.push({
					label: t("sidebar.switchBranch", "Switch Branch"),
					action: () => {},
					children: branchChildren,
				});
			}
		}

		// Group 2 — git branch lifecycle ("Branch ›" submenu).
		const git: ContextMenuItem[] = [];
		if (!isShell) {
			const branchOps: ContextMenuItem[] = [];
			if (props.onCreateBranch) {
				branchOps.push({ label: "Create Branch…", action: props.onCreateBranch });
			}
			branchOps.push({
				label: isLinkedWorktree ? "Rename Worktree" : "Rename Branch",
				action: props.onRename,
				disabled: props.branch.isMain,
			});
			if (!props.branch.isMain && !props.branch.worktreePath && props.onCreateWorktreeFromBranch) {
				branchOps.push({ label: "Create Worktree", action: props.onCreateWorktreeFromBranch });
			}
			if (!props.branch.isMain && isLinkedWorktree && props.onMergeAndArchive) {
				branchOps.push({ label: "Merge & Archive", action: props.onMergeAndArchive });
			}
			if (!props.branch.isMain && isLinkedWorktree && props.canRemove) {
				branchOps.push({
					label: props.isRemoving ? "Removing…" : "Delete Worktree",
					action: props.onRemove,
					disabled: props.isRemoving,
				});
			}
			if (branchOps.length > 0) {
				git.push({ label: "Branch", action: () => {}, children: branchOps });
			}
		}

		// Group 3 — PR / workflow actions (plugin + built-in smart prompts).
		const workflow: ContextMenuItem[] = [];
		const branchActions = contextMenuActionsStore.getContextActions("branch");
		if (branchActions.length > 0) {
			const ctx = { target: "branch" as const, repoPath: props.repoPath, branchName: props.branch.branchName };
			for (const a of branchActions) {
				workflow.push({ label: a.label, action: () => a.action(ctx), disabled: a.disabled?.(ctx) });
			}
		}

		// Group 4 — metadata (least frequent).
		const meta: ContextMenuItem[] = [];
		if (hasBranch) {
			meta.push({ label: "Set Label", action: () => props.onSetLabel?.(branchLabel()) });
			if (branchLabel()) {
				meta.push({
					label: "Clear Label",
					action: () => repoSettingsStore.setLabel(props.repoPath, props.branch.branchName, null),
				});
			}
		}

		// Flatten, inserting a separator before each non-empty group after the first.
		const out: ContextMenuItem[] = [];
		for (const group of [quick, git, workflow, meta]) {
			if (group.length === 0) continue;
			if (out.length > 0) group[0] = { ...group[0], separator: true };
			out.push(...group);
		}
		return out;
	};

	const branchIcon = () => (
		<BranchIcon
			isMainBranch={props.branch.isMain}
			isMainWorktree={props.branch.worktreePath === props.repoPath}
			isShell={props.branch.isShell}
			hasError={hasError()}
			hasQuestion={hasQuestion()}
			hasBusy={hasBusy()}
			hasUnseen={hasUnseen()}
			branchHasTerminals={props.branch.terminals.length > 0}
		/>
	);

	const isPendingOp = () => props.branch.isRemoving;
	const pendingLabel = () => "Removing…";

	return (
		<Show
			when={!isPendingOp()}
			fallback={
				<div
					class={cx(s.branchItem, s.branchPreparing)}
					aria-busy="true"
					aria-label={`${pendingLabel()} ${props.branch.branchName}`}
				>
					<BranchIcon
						isMainBranch={false}
						isMainWorktree={false}
						isShell={false}
						hasError={false}
						hasQuestion={false}
						hasBusy={true}
						hasUnseen={false}
						branchHasTerminals={true}
					/>
					<div class={s.branchContent}>
						<span class={s.branchName} style={{ opacity: "0.5" }}>
							{props.branch.branchName}
						</span>
						<span class={b.subLabel}>{pendingLabel()}</span>
					</div>
				</div>
			}
		>
			<div
				class={cx(s.branchItem, props.isActive && s.active)}
				onClick={() => props.onSelect()}
				onContextMenu={ctxMenu.open}
			>
				{/* The icon doubles as the agents toggle: it turns into a chevron on
				    hover, so expanding costs no width in the row. */}
				<Show when={getBranchTabsAvailable(props.branch)} fallback={branchIcon()}>
					<span
						class={cx(s.branchIconToggle, !props.branch.tabsCollapsed && s.expanded)}
						role="button"
						tabIndex={0}
						aria-expanded={!props.branch.tabsCollapsed}
						aria-label={`${t("sidebar.toggleAgents", "Show or hide agents")} (${props.branch.terminals.length})`}
						title={t("sidebar.toggleAgents", "Show or hide agents")}
						onClick={toggleAgents}
						onKeyDown={onClickKeyDown(toggleAgents)}
					>
						{branchIcon()}
						<span class={s.branchIconChevron}>
							<ChevronIcon />
						</span>
						{/* Rides the icon's corner so the count costs no width; it is the
						    only trace of the sessions while the list is collapsed. Expanded,
						    the rows themselves show them, so the count is only noise. */}
						<Show when={props.branch.tabsCollapsed}>
							<span class={s.branchAgentCount} aria-hidden="true">
								{props.branch.terminals.length}
							</span>
						</Show>
					</span>
				</Show>
				<div class={s.branchContent}>
					<span class={s.branchName} onDblClick={handleDoubleClick} title={rowTitle()}>
						{branchLabel() ?? props.branch.branchName}
					</span>
					{/* When a custom label replaces the main line, retain the branch
					    underneath it so Git-facing identity remains visible. */}
					<Show when={branchLabel()}>
						<span class={b.subLabel} title={rowTitle()}>
							{props.branch.branchName}
						</span>
					</Show>
				</div>
				{/* The badge answers one question — what would removing this workspace
				    lose? A main checkout is never removed here, so it gets no badge at
				    all: "Dirty" on every main row was noise about a risk that does not
				    exist. */}
				<Show when={!props.branch.isMain && props.branch.lifecycleStatus}>
					{(status) => {
						const lostFiles = () => status().dirtyFiles ?? 0;
						const label = () => {
							if (status().commitStatus === "unknown") return "Unknown";
							// Uncommitted files outrank the commit verdict, which reads commits
							// only: "Merged" over them claims nothing would be lost while removal
							// discards every one. The count earns a chip only when the stats chip
							// is absent — with both, one row carried the same warning twice — and
							// otherwise rides that chip's tooltip.
							if (lostFiles() > 0) {
								// One chip per row. Where the stats chip or the PR badge already
								// holds it, the count rides their tooltip instead of stacking a
								// second chip next to them.
								const otherChip = props.branch.additions + props.branch.deletions > 0 || !!pr();
								return otherChip ? null : `${lostFiles()} dirty`;
							}
							if (status().commitStatus === "merged") return "Merged";
							return null;
						};
						// Only the dirty count points at a diff; commit verdicts have none to show.
						const opensChanges = () => !!props.onShowChanges && status().commitStatus !== "unknown" && lostFiles() > 0;
						const tooltip = () => {
							if (status().commitStatus === "unknown") {
								const explanation =
									"Status unavailable: TUICommander could not verify local changes or merge state, so removal is blocked.";
								return status().error ? `${explanation} ${status().error}` : explanation;
							}
							if (lostFiles() > 0) {
								return `${lostFiles()} uncommitted file${lostFiles() === 1 ? "" : "s"} (staged, unstaged or untracked) would be discarded by removing this workspace.`;
							}
							const commitState =
								status().commitStatus === "merged"
									? "HEAD is merged"
									: status().commitStatus === "in_sync"
										? "HEAD is the default branch tip — no commits of its own"
										: "Branch has commits not merged into the default branch";
							const removal =
								status().commitStatus === "unmerged"
									? "merge before deleting the branch"
									: status().removalSafety === "safe"
										? "safe to remove"
										: "destructive confirmation required";
							return `Clean working tree; ${commitState}; ${removal}`;
						};
						return (
							<Show when={label()}>
								<span
									class={`${s.lifecycleBadge} ${
										status().removalSafety !== "safe" ? s.lifecycleRisk : s.lifecycleMerged
									}`}
									data-tooltip={tooltip()}
									data-tooltip-pos="bottom"
									data-tooltip-align="right"
									tabIndex={0}
									role={opensChanges() ? "button" : undefined}
									onClick={opensChanges() ? showChanges : undefined}
									onKeyDown={opensChanges() ? onClickKeyDown(showChanges) : undefined}
									style={opensChanges() ? { cursor: "pointer" } : undefined}
								>
									{label()}
								</span>
							</Show>
						);
					}}
				</Show>
				<Show when={props.branch.lifecycleStatus?.commitStatus === "unmerged" || pr() || hasDiff()}>
					<div class={s.branchBadgeStack}>
						<Show when={props.branch.lifecycleStatus?.commitStatus === "unmerged"}>
							<span
								class={s.branchUnmergedMarker}
								aria-label="Unmerged commits"
								data-tooltip="Branch has commits not merged into the default branch. Merge before deleting the branch."
								data-tooltip-pos="bottom"
								data-tooltip-align="right"
								tabIndex={0}
							>
								<span class={s.branchUnmergedGlyph} aria-hidden="true" />
							</span>
						</Show>
						<Show when={pr()}>
							<span
								class={(() => {
									const st = pr()?.state?.toLowerCase();
									return st === "closed" || st === "merged" ? s.prBadgeDimmed : undefined;
								})()}
								onClick={(e) => {
									e.stopPropagation();
									props.onShowPrDetail();
								}}
							>
								<PrStateBadge
									compact
									prNumber={pr()!.number}
									state={pr()!.state}
									isDraft={pr()!.is_draft}
									mergeable={pr()!.mergeable}
									conflictState={pr()!.conflict_state}
									reviewDecision={pr()!.review_decision}
									ciPassed={checks()?.passed}
									ciFailed={checks()?.failed}
									ciPending={checks()?.pending}
									dirtyFiles={props.branch.lifecycleStatus?.dirtyFiles ?? undefined}
								/>
							</span>
						</Show>
						<StatsBadge
							additions={props.branch.additions}
							deletions={props.branch.deletions}
							dirtyFiles={props.branch.lifecycleStatus?.dirtyFiles ?? undefined}
							onClick={props.onShowChanges ? showChanges : undefined}
						/>
					</div>
				</Show>
				<div class={s.branchActions} style={{ display: props.shortcutIndex !== undefined ? "none" : undefined }}>
					<button
						class={s.branchAddBtn}
						onClick={(e) => {
							e.stopPropagation();
							props.onAddTerminal();
						}}
						title={t("sidebar.addTerminal", "Add terminal")}
					>
						+
					</button>
					{/* Only linked worktrees can be removed — never the main checkout, whose
					    worktreePath IS the repo root. `isMain` is name-based (main/master/
					    develop) so it misses a main checkout sitting on a differently-named
					    branch; the worktreePath !== repoPath test is the reliable signal and
					    mirrors the context-menu `isLinkedWorktree` predicate. */}
					<Show
						when={
							!props.branch.isMain &&
							props.branch.worktreePath &&
							props.branch.worktreePath !== props.repoPath &&
							props.canRemove
						}
					>
						<button
							class={s.branchRemoveBtn}
							disabled={props.isRemoving}
							onClick={(e) => {
								e.stopPropagation();
								props.onRemove();
							}}
							title={
								props.isRemoving
									? t("sidebar.removingWorktree", "Removing…")
									: t("sidebar.removeWorktree", "Remove worktree")
							}
						>
							{props.isRemoving ? "…" : "×"}
						</button>
					</Show>
				</div>
				<span class={s.branchShortcut} style={{ display: props.shortcutIndex !== undefined ? undefined : "none" }}>
					{props.shortcutIndex !== undefined ? keyFor(`switch-branch-${props.shortcutIndex}`) : ""}
				</span>
				<ContextMenu
					items={contextMenuItems()}
					x={ctxMenu.position().x}
					y={ctxMenu.position().y}
					visible={ctxMenu.visible()}
					onClose={ctxMenu.close}
				/>
			</div>
		</Show>
	);
};

export { PrStateBadge } from "./PrStateBadge";
export { canMergePr } from "./prMergeEligibility";

import { GitHubPanel } from "./GitHubPanel";

/** Repository section component */
export const RepoSection: Component<{
	repo: RepositoryState;
	nameColor?: string;
	isDragging?: boolean;
	dragOverClass?: string;
	isCreatingWorktree?: boolean;
	removingBranches?: Set<string>;
	quickSwitcherActive?: boolean;
	branchShortcutStart: number;
	onBranchSelect: (branchName: string) => void;
	onAddTerminal: (branchName: string) => void;
	onRemoveBranch: (branchName: string) => void;
	onRenameBranch: (branchName: string) => void;
	onCreateBranch?: (fromBranch: string) => void;
	onShowPrDetail: (branchName: string) => void;
	onShowChanges?: () => void;
	buildAgentMenuItems?: (branchName: string) => ContextMenuItem[];
	onAddWorktree: () => void;
	onCreateWorktreeFromBranch?: (branchName: string) => void;
	onMergeAndArchive?: (branchName: string) => void;
	onSettings: () => void;
	onRemove: () => void;
	onToggle: () => void;
	onToggleCollapsed: () => void;
	onCheckoutRemoteBranch?: (branchName: string) => void;
	onAutofixIssue?: (issueNumber: number, prompt: string) => void;
	onConflictAssist?: (prNumber: number) => void;
	onPushBranch?: (worktreePath: string) => void;
	onSwitchBranch: (branchName: string) => void;
	switchBranchList: () => string[];
	currentBranch: () => string;
	onMouseDrag: (e: PointerEvent) => void;
}> = (props) => {
	const repoMenu = createContextMenu();
	const [labelDialogBranch, setLabelDialogBranch] = createSignal<{ name: string; current: string | undefined } | null>(
		null,
	);
	const [groupPromptVisible, setGroupPromptVisible] = createSignal(false);
	const [remoteOnlyPopoverVisible, setRemoteOnlyPopoverVisible] = createSignal(false);
	const [remoteCleanupActive, setRemoteCleanupActive] = createSignal(false);
	const [githubBaseUrl, setGithubBaseUrl] = createSignal<string | null>(null);

	// Fetch GitHub URL for "Open in GitHub" context menu actions
	if (props.repo.isGitRepo !== false) {
		invoke<string | null>("get_remote_url", { path: props.repo.path })
			.then((url) => {
				if (url) setGithubBaseUrl(remoteUrlToGitHub(url));
			})
			.catch(() => {});
	}

	const branches = createMemo(() => Object.values(props.repo.workspaces));
	// Pre-compute PR statuses once per poll cycle; avoids calling getPrStatus inside sort comparator
	const prStatuses = createMemo(() => {
		const map = new Map<string, ReturnType<typeof githubStore.getPrStatus>>();
		for (const b of branches()) {
			map.set(b.branchName, githubStore.getPrStatus(props.repo.path, b.branchName));
		}
		return map;
	});
	const sortedBranches = createMemo(() =>
		// Freeze-investigation: this re-sort + the <For> reconcile below is the
		// leading suspect for the git.refreshBatch flush cost — setWorkspace creates a
		// new branch object ref on every repo-changed, waking this memo even when
		// nothing structural changed. timeSync is dormant unless perfDebug is on.
		timeSync(`sidebar.sortedBranches:${props.repo.path}`, () => {
			const statuses = prStatuses();
			return [...branches()].sort((a, b) =>
				compareBranches(a, b, statuses.get(a.branchName), statuses.get(b.branchName)),
			);
		}),
	);
	const canRemoveAny = createMemo(() => sortedBranches().length > 1);

	const localBranchNames = createMemo(() => new Set(Object.keys(props.repo.workspaces)));
	const remoteOnlyPrs = createMemo(() => githubStore.getRemoteOnlyPrs(props.repo.path, localBranchNames()));
	const allOpenPrs = createMemo(() => githubStore.getAllOpenPrs(props.repo.path));
	const repoIssues = createMemo(() => githubStore.getRepoIssues(props.repo.path));
	const viewerLogin = () => githubStore.state.viewerLogin;
	const myPrsCount = createMemo(() => {
		const login = viewerLogin();
		if (!login) return 0;
		return allOpenPrs().filter((pr) => pr.author === login).length;
	});
	const otherCount = createMemo(() => {
		const login = viewerLogin();
		const otherPrs = login ? remoteOnlyPrs().filter((pr) => pr.author !== login).length : remoteOnlyPrs().length;
		return otherPrs + repoIssues().length;
	});
	const ghBadgeCount = createMemo(() => myPrsCount() + otherCount());

	/**
	 * The badge on a remote repo reports its machine's live state, not just the
	 * fact that it is remote. An unreachable machine makes every operation on
	 * this repo fail, and that cause has to be visible where the repo is — not
	 * only in Settings, which is where it used to be the only place to find it.
	 */
	const remoteConnState = createMemo(() =>
		props.repo.connectionId ? remoteConnectionsStore.getConnectionState(props.repo.connectionId) : undefined,
	);
	const remoteBadgeLabel = createMemo(() => {
		const status = remoteConnState()?.status;
		if (status === "connected") return "remote";
		if (status === "connecting") return "connecting…";
		return "offline";
	});
	const remoteBadgeStatusClass = createMemo(() => {
		const status = remoteConnState()?.status;
		if (status === "connected") return undefined;
		if (status === "connecting") return s.remoteBadgeConnecting;
		return s.remoteBadgeError;
	});
	const remoteBadgeTitle = createMemo(() => {
		const state = remoteConnState();
		if (!state) return "This repository lives on a remote machine.";
		if (state.status === "connected") return `On ${state.connection.name}.`;
		const reason =
			state.status === "unauthenticated"
				? "rejected these credentials"
				: state.status === "connecting"
					? "is still connecting"
					: state.status === "error"
						? "is not answering"
						: "is not connected";
		return `${state.connection.name} ${reason}. Reconnect in Settings → Remote Machines.`;
	});

	const repoMenuItems = (): ContextMenuItem[] => {
		const items: ContextMenuItem[] = [{ label: "Repo Settings", action: () => props.onSettings() }];

		if (props.repo.isGitRepo !== false) {
			items.push({ label: "Create Worktree", action: () => props.onAddWorktree() });
		}

		// "Move to Group" submenu — always available (includes "New Group...")
		const layout = repositoriesStore.getGroupedLayout();
		const currentGroup = repositoriesStore.getGroupForRepo(props.repo.path);
		const children: ContextMenuItem[] = layout.groups
			.filter((entry) => entry.group.id !== currentGroup?.id)
			.map((entry) => ({
				label: entry.group.name,
				action: () => repositoriesStore.addRepoToGroup(props.repo.path, entry.group.id),
			}));
		if (currentGroup) {
			children.push({
				label: "Ungrouped",
				action: () => repositoriesStore.removeRepoFromGroup(props.repo.path),
			});
		}
		children.push({
			separator: children.length > 0,
			label: "New Group\u2026",
			action: () => setGroupPromptVisible(true),
		});
		items.push({ label: "Move to Group", action: () => {}, children });

		// GitHub link
		const ghUrl = githubBaseUrl();
		if (ghUrl) {
			items.push({ label: "Open in GitHub", action: () => handleOpenUrl(ghUrl), separator: true });
		}
		items.push({
			label: "Park Repository",
			action: () => repositoriesStore.setPark(props.repo.path, true),
			separator: !ghUrl,
		});
		items.push({ label: "Remove Repository", action: () => props.onRemove() });
		// Plugin-registered repo actions
		const repoActions = contextMenuActionsStore.getContextActions("repo");
		if (repoActions.length > 0) {
			const ctx = { target: "repo" as const, repoPath: props.repo.path };
			for (const a of repoActions) {
				items.push({
					label: a.label,
					action: () => a.action(ctx),
					disabled: a.disabled?.(ctx),
					separator: repoActions.indexOf(a) === 0,
				});
			}
		}
		return items;
	};

	const handleMenuToggle = (e: MouseEvent) => {
		e.stopPropagation();
		if (repoMenu.visible()) {
			repoMenu.close();
		} else {
			// Position below the button
			const btn = e.currentTarget as HTMLElement;
			const rect = btn.getBoundingClientRect();
			repoMenu.open({ preventDefault: () => {}, clientX: rect.right - 160, clientY: rect.bottom + 4 } as MouseEvent);
		}
	};

	return (
		<div
			class={cx(
				s.repoSection,
				props.repo.collapsed && s.collapsed,
				props.isDragging && s.dragging,
				props.dragOverClass,
			)}
			data-sidebar-repo={props.repo.path}
			onPointerDown={(e) => props.onMouseDrag(e)}
		>
			{/* Repo header */}
			<div
				class={s.repoHeader}
				role="button"
				tabIndex={0}
				onClick={props.onToggle}
				onKeyDown={onClickKeyDown(props.onToggle)}
				onContextMenu={repoMenu.open}
			>
				<Show when={props.repo.collapsed}>
					<span
						class={s.repoInitials}
						onClick={(e) => {
							e.stopPropagation();
							props.onToggleCollapsed();
						}}
						title={t("sidebar.clickToExpand", "Click to expand")}
					>
						{props.repo.initials}
					</span>
				</Show>
				<Show when={!props.repo.collapsed}>
					<span class={s.repoName} style={props.nameColor ? { color: props.nameColor } : undefined}>
						{props.repo.displayName}
					</span>
					<Show when={props.repo.connectionId}>
						<span class={cx(s.remoteBadge, remoteBadgeStatusClass())} title={remoteBadgeTitle()}>
							{remoteBadgeLabel()}
						</span>
					</Show>
					<div class={cx(s.repoActions, ghBadgeCount() > 0 && s.repoActionsWithBadge)}>
						<button
							class={s.repoActionBtn}
							onClick={handleMenuToggle}
							title={t("sidebar.repoOptions", "Repository options")}
						>
							⋯
						</button>
						{/* Non-git repos have no worktrees, so the add button is absent. The
						    fallback keeps its slot occupied — without it the whole trailing
						    cluster (⋯ + chevron) would shift right and stop lining up with
						    the git-repo headers above and below it. */}
						<Show
							when={props.repo.isGitRepo !== false}
							fallback={<span class={cx(s.repoActionBtn, s.addBtn, s.repoActionSlot)} aria-hidden="true" />}
						>
							<button
								class={cx(s.repoActionBtn, s.addBtn)}
								disabled={props.isCreatingWorktree}
								onClick={(e) => {
									e.stopPropagation();
									props.onAddWorktree();
								}}
								title={
									props.isCreatingWorktree
										? t("sidebar.creatingWorktree", "Creating worktree…")
										: t("sidebar.addWorktree", "Add worktree")
								}
							>
								{props.isCreatingWorktree ? "…" : "+"}
							</button>
						</Show>
						{/* Last in the cluster, next to the chevron: the hover-only ⋯ and + keep
						    their boxes while invisible, so a badge ahead of them floated in a gap. */}
						<Show when={ghBadgeCount() > 0}>
							<button
								class={cx(s.repoActionBtn, s.ghBadgeBtn)}
								onClick={(e) => {
									e.stopPropagation();
									setRemoteOnlyPopoverVisible((v) => !v);
								}}
								title={t("sidebar.githubPanelTitle", "GitHub: PRs & Issues")}
							>
								<svg width="10" height="10" viewBox="0 0 16 16" fill="currentColor">
									<path
										fill-rule="evenodd"
										d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z"
									/>
								</svg>
								<Show when={myPrsCount() > 0 && otherCount() > 0}>
									{myPrsCount()}
									<span class={s.ghBadgeSep}>◈</span>
									{otherCount()}
								</Show>
								<Show when={myPrsCount() > 0 && otherCount() === 0}>{myPrsCount()}</Show>
								<Show when={myPrsCount() === 0}>{otherCount()}</Show>
							</button>
						</Show>
					</div>
					<span class={cx(s.repoChevron, props.repo.expanded && s.expanded)}>
						<ChevronIcon />
					</span>
				</Show>
			</div>

			{/* Branches */}
			<Show when={props.repo.expanded && !props.repo.collapsed}>
				<div class={s.repoBranches}>
					<For each={sortedBranches()}>
						{(branch, index) => (
							<div
								class={cx(
									s.branchGroup,
									!branch.tabsCollapsed && getBranchTabsAvailable(branch) && s.branchGroupExpanded,
								)}
							>
								<BranchItem
									branch={branch}
									repoPath={props.repo.path}
									isActive={
										repositoriesStore.state.activeRepoPath === props.repo.path &&
										props.repo.activeWorkspaceId === branch.workspaceId
									}
									canRemove={canRemoveAny()}
									shortcutIndex={props.quickSwitcherActive ? props.branchShortcutStart + index() : undefined}
									// Selecting, adding a terminal to, or removing a row all address
									// the WORKSPACE — the row is what the user clicked, and two rows
									// may share a branch. Everything below that names a git ref
									// (rename, create-from, switch, PR) still passes `branchName`.
									agentMenuItems={
										props.buildAgentMenuItems ? () => props.buildAgentMenuItems!(branch.workspaceId) : undefined
									}
									onSelect={() => props.onBranchSelect(branch.workspaceId)}
									onAddTerminal={() => props.onAddTerminal(branch.workspaceId)}
									isRemoving={props.removingBranches?.has(`${props.repo.path}::${branch.workspaceId}`)}
									onRemove={() => props.onRemoveBranch(branch.workspaceId)}
									onRename={() => props.onRenameBranch(branch.branchName)}
									onCreateBranch={props.onCreateBranch ? () => props.onCreateBranch!(branch.branchName) : undefined}
									onSetLabel={(current) => setLabelDialogBranch({ name: branch.branchName, current })}
									onShowPrDetail={() => props.onShowPrDetail(branch.branchName)}
									onShowChanges={props.onShowChanges}
									onCreateWorktreeFromBranch={
										props.onCreateWorktreeFromBranch
											? () => props.onCreateWorktreeFromBranch!(branch.branchName)
											: undefined
									}
									onMergeAndArchive={
										props.onMergeAndArchive ? () => props.onMergeAndArchive!(branch.workspaceId) : undefined
									}
									onSwitchBranch={
										branch.worktreePath === props.repo.path ? (name) => props.onSwitchBranch(name) : undefined
									}
									switchBranchList={branch.worktreePath === props.repo.path ? props.switchBranchList : undefined}
									currentBranch={branch.worktreePath === props.repo.path ? props.currentBranch : undefined}
									githubBaseUrl={githubBaseUrl()}
								/>
								<Show when={!branch.tabsCollapsed && getBranchTabsAvailable(branch)}>
									<BranchTabList terminalIds={branch.terminals} />
								</Show>
							</div>
						)}
					</For>
					<Show when={sortedBranches().length === 0}>
						<div class={s.repoEmpty}>{t("sidebar.noBranches", "No branches loaded")}</div>
					</Show>
				</div>
			</Show>
			<Show when={props.repo.expanded && !props.repo.collapsed}>
				<For each={sidebarPluginStore.getPanels().filter((p) => p.items.length > 0)}>
					{(panel) => <SidebarPluginSection panel={panel} />}
				</For>
			</Show>
			<ContextMenu
				items={repoMenuItems()}
				x={repoMenu.position().x}
				y={repoMenu.position().y}
				visible={repoMenu.visible()}
				onClose={repoMenu.close}
			/>
			<PromptDialog
				visible={!!labelDialogBranch()}
				title="Set Label"
				placeholder="Human-readable name…"
				confirmLabel="Save"
				maxLength={60}
				defaultValue={labelDialogBranch()?.current ?? ""}
				subtitle={labelDialogBranch()?.name}
				onClose={() => setLabelDialogBranch(null)}
				onConfirm={(label) => {
					const br = labelDialogBranch();
					if (br) repoSettingsStore.setLabel(props.repo.path, br.name, label || null);
					setLabelDialogBranch(null);
				}}
			/>
			<PromptDialog
				visible={groupPromptVisible()}
				title="New Group"
				placeholder="Group name"
				confirmLabel="Create"
				onClose={() => setGroupPromptVisible(false)}
				onConfirm={(name) => {
					const groupId = repositoriesStore.createGroup(name);
					if (groupId) {
						repositoriesStore.addRepoToGroup(props.repo.path, groupId);
					}
				}}
			/>
			<Show when={remoteOnlyPopoverVisible() && (ghBadgeCount() > 0 || remoteCleanupActive())}>
				<GitHubPanel
					prs={remoteOnlyPrs()}
					allPrs={allOpenPrs()}
					repoPath={props.repo.path}
					onClose={() => setRemoteOnlyPopoverVisible(false)}
					onCheckout={(branch) => {
						setRemoteOnlyPopoverVisible(false);
						props.onCheckoutRemoteBranch?.(branch);
					}}
					onCreateWorktree={props.onCreateWorktreeFromBranch}
					onConflictAssist={props.onConflictAssist}
					onPushBranch={props.onPushBranch}
					onAutofix={props.onAutofixIssue}
					onCleanupActive={setRemoteCleanupActive}
				/>
			</Show>
		</div>
	);
};
