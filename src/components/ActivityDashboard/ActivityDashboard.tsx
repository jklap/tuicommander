import { type Component, createEffect, createMemo, createSignal, For, onCleanup, Show } from "solid-js";
import { activityDashboardStore } from "../../stores/activityDashboard";
import { globalWorkspaceStore } from "../../stores/globalWorkspace";
import { registerModal } from "../../stores/modalStack";
import { rateLimitStore } from "../../stores/ratelimit";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";
import {
	displayTask,
	effectiveActivityState,
	isActivityWorking,
	projectName,
	reconcileActivityOrder,
	terminalStatusLabel,
} from "../../utils/activitySnapshot";
import { navigateToTerminal } from "../../utils/navigateToTerminal";
import { getRepoColor } from "../../utils/repoColor";
import { formatRelativeTime } from "../../utils/time";
import { GlobeIcon } from "../GlobeIcon";
import { PanelWindowControls } from "../ui/PanelWindowControls";
import s from "./ActivityDashboard.module.css";

export const statusClasses = {
	rateLimited: s.statusRateLimited,
	error: s.statusError,
	waiting: s.statusWaiting,
	working: s.statusWorking,
	idle: s.statusIdle,
};

/** Truncate a string to a single line for display */
function truncate(text: string, maxLen = 80): string {
	const oneLine = text.replace(/\n/g, " ").trim();
	if (oneLine.length <= maxLen) return oneLine;
	return oneLine.slice(0, maxLen - 1) + "\u2026";
}

/** Speech bubble icon (last prompt) */
const PromptIcon: Component = () => (
	<svg class={s.subIcon} viewBox="0 0 16 16" width="12" height="12" fill="currentColor">
		<path d="M1 3.5A1.5 1.5 0 0 1 2.5 2h11A1.5 1.5 0 0 1 15 3.5v7A1.5 1.5 0 0 1 13.5 12H9.373l-2.62 1.81A.75.75 0 0 1 5.6 13.2V12H2.5A1.5 1.5 0 0 1 1 10.5v-7Zm1.5-.5a.5.5 0 0 0-.5.5v7a.5.5 0 0 0 .5.5H6.35a.75.75 0 0 1 .75.75v.83l1.81-1.25a.75.75 0 0 1 .427-.133H13.5a.5.5 0 0 0 .5-.5v-7a.5.5 0 0 0-.5-.5h-11Z" />
	</svg>
);

/** Crosshair icon (agent intent) */
const IntentIcon: Component = () => (
	<svg class={s.subIcon} viewBox="0 0 16 16" width="12" height="12" fill="currentColor">
		<path d="M8 1a.75.75 0 0 1 .75.75v1.82a4.505 4.505 0 0 1 3.68 3.68h1.82a.75.75 0 0 1 0 1.5h-1.82a4.505 4.505 0 0 1-3.68 3.68v1.82a.75.75 0 0 1-1.5 0v-1.82a4.505 4.505 0 0 1-3.68-3.68H1.75a.75.75 0 0 1 0-1.5h1.82A4.505 4.505 0 0 1 7.25 3.57V1.75A.75.75 0 0 1 8 1ZM5.5 8a2.5 2.5 0 1 0 5 0 2.5 2.5 0 0 0-5 0Z" />
	</svg>
);

/** Gear/spinner icon (current task) */
const TaskIcon: Component = () => (
	<svg class={s.subIcon} viewBox="0 0 16 16" width="12" height="12" fill="currentColor">
		<path d="M7.068.727c.243-.97 1.62-.97 1.864 0l.3 1.2a.957.957 0 0 0 1.18.633l1.18-.39c.93-.31 1.753.789 1.13 1.593l-.76.98a.957.957 0 0 0 .166 1.34l1.01.76c.78.59.39 1.82-.55 1.84l-1.22.03a.957.957 0 0 0-.905.905l-.03 1.22c-.02.94-1.25 1.33-1.84.55l-.76-1.01a.957.957 0 0 0-1.34-.166l-.98.76c-.804.623-1.903-.2-1.593-1.13l.39-1.18a.957.957 0 0 0-.633-1.18l-1.2-.3c-.97-.243-.97-1.62 0-1.864l1.2-.3a.957.957 0 0 0 .633-1.18l-.39-1.18c-.31-.93.789-1.753 1.593-1.13l.98.76a.957.957 0 0 0 1.34-.166l.76-1.01ZM8 11a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z" />
	</svg>
);

/** People icon (active sub-tasks / agents) */
const SubTaskIcon: Component = () => (
	<svg class={s.subIcon} viewBox="0 0 16 16" width="12" height="12" fill="currentColor">
		<path d="M2 5.5a3.5 3.5 0 1 1 5.898 2.549 5.508 5.508 0 0 1 3.034 4.084.75.75 0 1 1-1.482.235 4.001 4.001 0 0 0-7.9 0 .75.75 0 0 1-1.482-.236A5.507 5.507 0 0 1 3.102 8.05 3.493 3.493 0 0 1 2 5.5ZM11 4a.75.75 0 1 0 0 1.5 2.5 2.5 0 0 1 2.45 2.993.75.75 0 1 0 1.472.29A4.001 4.001 0 0 0 11 4Zm-5.5.5a2 2 0 1 0 0 4 2 2 0 0 0 0-4Z" />
	</svg>
);

export type TerminalRow = {
	id: string;
	name: string;
	project: string | null;
	projectColor: string | undefined;
	agent: string;
	status: { label: string; className: string };
	isWorking: boolean;
	lastDataAt: number | null;
	idleSince: number | null;
	lastPrompt: string | null;
	agentIntent: string | null;
	currentTask: string | null;
	activeSubTasks: number;
	isActive: boolean;
	isPromoted: boolean;
};

/** Field-equality for a row. `status` is derived fresh on every build, so it is
 *  the one member that must be compared by value rather than by reference. */
function sameRow(a: TerminalRow, b: TerminalRow): boolean {
	for (const key of Object.keys(a) as Array<keyof TerminalRow>) {
		if (key === "status") continue;
		if (a[key] !== b[key]) return false;
	}
	return a.status.label === b.status.label && a.status.className === b.status.className;
}

/** Rows are rendered through a reference-keyed `<For>`, and both producers
 *  (the store memo below, the 1 Hz snapshot in panelAdapters/activity.tsx)
 *  rebuild every row object from scratch on every tick — so an untouched
 *  terminal's DOM subtree was torn down and recreated once a second, forever.
 *  Hand the previous rows back in: unchanged ones keep their identity, and when
 *  nothing at all moved `prev` is returned itself so the `<For>` does not re-run. */
export function reconcileTerminalRows(next: TerminalRow[], prev?: readonly TerminalRow[]): TerminalRow[] {
	const prevById = prev ? new Map(prev.map((r) => [r.id, r])) : undefined;
	const rows = next.map((r) => {
		const old = prevById?.get(r.id);
		return old && sameRow(old, r) ? old : r;
	});
	// Positional, not just per-id: two reused rows that swapped places are still
	// a changed list, and handing back `prev` would silently keep the old order.
	if (prev && prev.length === rows.length && rows.every((r, i) => r === prev[i])) {
		return prev as TerminalRow[];
	}
	return rows;
}

/** Move a keyboard selection cursor over `ids`. From no selection, "down" (delta=1)
 *  picks the first row and "up" (delta=-1) picks the last — matching CommandPalette /
 *  GitHubPanel. Clamps at both ends; never wraps. Returns `current` unchanged if it is
 *  no longer present (caller is responsible for clearing a stale selection first) other
 *  than the no-selection case, which always resolves to an end of the list. */
export function moveActivitySelection(ids: readonly string[], current: string | null, delta: 1 | -1): string | null {
	if (ids.length === 0) return null;
	if (current === null) return delta > 0 ? ids[0] : ids[ids.length - 1];
	const i = ids.indexOf(current);
	if (i === -1) return delta > 0 ? ids[0] : ids[ids.length - 1];
	const next = i + delta;
	if (next < 0 || next >= ids.length) return ids[i];
	return ids[next];
}

interface ActivityDashboardProps {
	onSelect?: (id: string) => void;
	onPromote?: (id: string) => void;
	/** When true, renders without overlay — used in detached panel windows. */
	embedded?: boolean;
	/** External data source. When provided, bypasses store reads. */
	terminals?: () => TerminalRow[];
}

export const ActivityDashboard: Component<ActivityDashboardProps> = (props) => {
	const [, setTick] = createSignal(0);
	const isOpen = () => activityDashboardStore.state.isOpen;

	// Tick every second to refresh relative timestamps
	createEffect(() => {
		if (!isOpen()) return;
		const interval = setInterval(() => setTick((n) => n + 1), 1000);
		onCleanup(() => clearInterval(interval));
	});

	// Escape-to-close is handled centrally (stores/modalStack): registering routes
	// Escape to the dashboard close AND stops it reaching the terminal underneath.
	createEffect(() => {
		if (!isOpen()) return;
		registerModal(() => activityDashboardStore.close());
	});

	const handleRowClick = (termId: string) => {
		if (props.onSelect) {
			props.onSelect(termId);
		} else {
			navigateToTerminal(termId);
		}
		if (!props.embedded) activityDashboardStore.close();
	};

	/** Build a fresh row from the live store. Called at render time so every
	 *  store mutation (intent, status, last-prompt, …) flows through immediately
	 *  instead of waiting for the 10s order snapshot. */
	const buildRow = (id: string): TerminalRow | null => {
		const term = terminalsStore.get(id);
		if (!term) return null;
		const isRL = !!(term.sessionId && rateLimitStore.isRateLimited(term.sessionId));
		const effectiveState = effectiveActivityState(
			term.shellState,
			term.awaitingInput,
			isRL,
			term.agentState,
			term.backgroundWork,
		);
		const status = terminalStatusLabel(
			term.shellState,
			term.awaitingInput,
			isRL,
			statusClasses,
			term.agentState,
			term.backgroundWork,
		);
		const repoPath = repositoriesStore.getRepoPathForTerminal(id);
		return {
			id,
			name: term.name,
			project: projectName(term.cwd),
			projectColor: repoPath ? getRepoColor(repoPath) : undefined,
			agent: term.agentType || "shell",
			status,
			isWorking: isActivityWorking(effectiveState),
			lastDataAt: terminalsStore.getLastDataAt(id),
			idleSince: term.idleSince,
			lastPrompt: term.lastPrompt,
			agentIntent: term.agentIntent,
			currentTask: displayTask(term.currentTask, term.agentType),
			activeSubTasks: term.activeSubTasks,
			isActive: terminalsStore.state.activeId === id,
			isPromoted: globalWorkspaceStore.isPromoted(id),
		};
	};

	// Persistent display spine: each terminal keeps its slot so rows never
	// reshuffle while their working/idle state is unchanged. A working terminal moves
	// ONLY when it crosses the working/idle boundary (a real state change); an idle
	// terminal is additionally ordered by idleSince (most-recently-active first), which
	// itself only changes on that same boundary crossing — so there is still no
	// recency-tick reshuffling.
	const spine: string[] = [];
	const orderedIds = createMemo(() => {
		const isWorking = (id: string): boolean => {
			const term = terminalsStore.get(id);
			const isRL = !!(term?.sessionId && rateLimitStore.isRateLimited(term.sessionId));
			return (
				!!term &&
				isActivityWorking(
					effectiveActivityState(term.shellState, term.awaitingInput, isRL, term.agentState, term.backgroundWork),
				)
			);
		};
		const idleSortKey = (id: string): number | null => terminalsStore.get(id)?.idleSince ?? null;
		return reconcileActivityOrder(spine, terminalsStore.getAttachedIds(), isWorking, idleSortKey);
	});

	let prevRows: TerminalRow[] | undefined;
	const storeTerminals = createMemo(() => {
		prevRows = reconcileTerminalRows(orderedIds().map(buildRow).filter(Boolean) as TerminalRow[], prevRows);
		return prevRows;
	});

	const terminals = () => (props.terminals ? props.terminals() : storeTerminals());

	// Keyboard cursor, tracked by terminal id rather than row index: the list itself
	// reorders (a working/idle transition, an idle row's spot in the idleSince sort)
	// independently of any keypress, so an index cursor could silently slide onto a
	// different terminal between a keypress and the following Return.
	const [selectedId, setSelectedId] = createSignal<string | null>(null);
	let listRef: HTMLDivElement | undefined;

	createEffect(() => {
		if (!isOpen() && !props.embedded) setSelectedId(null);
	});

	// Drop the selection if its terminal leaves the list (closed, detached, etc.) —
	// "no selection" is already a first-class state, so there is nothing else to reconcile.
	createEffect(() => {
		const ids = terminals().map((t) => t.id);
		if (selectedId() !== null && !ids.includes(selectedId() as string)) setSelectedId(null);
	});

	createEffect(() => {
		const id = selectedId();
		if (!id || !listRef) return;
		listRef.querySelector(`[data-term-id="${CSS.escape(id)}"]`)?.scrollIntoView({ block: "nearest" });
	});

	// Arrow/Enter/digit navigation is local to the dashboard, following the same split
	// CommandPalette/BranchSwitcher use: Escape stays owned by the central modalStack
	// (registered above), everything else is a local capture-phase listener so it can
	// preventDefault + stopPropagation before useKeyboardRedirect's bubble-phase listener
	// ever sees the key (bare digits are otherwise plain printable characters that would
	// get written straight to the active terminal's PTY).
	createEffect(() => {
		if (!isOpen() && !props.embedded) return;

		const handleKeydown = (e: KeyboardEvent) => {
			const ids = terminals().map((t) => t.id);

			if (e.key === "ArrowDown" || e.key === "ArrowUp") {
				e.preventDefault();
				e.stopPropagation();
				setSelectedId(moveActivitySelection(ids, selectedId(), e.key === "ArrowDown" ? 1 : -1));
				return;
			}

			if (e.key === "Enter") {
				const id = selectedId();
				if (id && ids.includes(id)) {
					e.preventDefault();
					e.stopPropagation();
					handleRowClick(id);
				}
				return;
			}

			// Bare digits 1-9 jump straight to that row. Modified digits (Cmd+1, Cmd+Ctrl+1,
			// …) are left alone — those are the global switch-tab-N / switch-branch-N
			// shortcuts and must keep working while the dashboard is open.
			if (!e.metaKey && !e.ctrlKey && !e.altKey && e.key >= "1" && e.key <= "9") {
				e.preventDefault();
				e.stopPropagation();
				const id = ids[Number(e.key) - 1];
				if (id) {
					setSelectedId(id);
					handleRowClick(id);
				}
			}
		};

		document.addEventListener("keydown", handleKeydown, true);
		onCleanup(() => document.removeEventListener("keydown", handleKeydown, true));
	});

	const dashboardContent = () => (
		<>
			<div class={s.header}>
				<h3>Activity Dashboard</h3>
				<div class={s.headerActions}>
					<PanelWindowControls
						panelId="activity"
						mode={props.embedded ? "detached" : "inline"}
						onInlineClose={() => activityDashboardStore.close()}
					/>
				</div>
			</div>

			<div class={s.list} ref={listRef}>
				<Show when={terminals().length === 0}>
					<div class={s.empty}>No active terminals</div>
				</Show>

				<For each={terminals()}>
					{(term, index) => (
						<div
							class={`${s.row} ${term.isActive ? s.activeRow : ""} ${!term.isWorking ? s.idleRow : ""} ${term.id === selectedId() ? s.selectedRow : ""}`}
							data-term-id={term.id}
							onClick={() => handleRowClick(term.id)}
						>
							<div class={s.rowMain}>
								<div class={s.nameCell}>
									<Show when={index() < 9}>
										<span class={s.rowNumber}>{index() + 1}</span>
									</Show>
									<span class={s.termName}>{term.name}</span>
									<Show when={term.project}>
										<span class={s.project} style={term.projectColor ? { color: term.projectColor } : undefined}>
											{term.project}
										</span>
									</Show>
								</div>
								<span class={s.agent}>{term.agent}</span>
								<span class={`${s.status} ${term.status.className}`}>{term.status.label}</span>
								<span class={s.lastActivity}>{term.isWorking ? "" : formatRelativeTime(term.idleSince)}</span>
								<button
									class={`${s.promoteBtn} ${term.isPromoted ? s.promoted : ""}`}
									title={term.isPromoted ? "Remove from Global Workspace" : "Promote to Global Workspace"}
									onClick={(e) => {
										e.stopPropagation();
										if (props.onPromote) {
											props.onPromote(term.id);
										} else {
											globalWorkspaceStore.togglePromote(term.id);
										}
									}}
								>
									<GlobeIcon />
								</button>
							</div>
							<Show when={term.currentTask}>
								{(task) => (
									<div class={s.subRow} title={task()}>
										<TaskIcon />
										<span class={s.subText}>{truncate(task())}</span>
									</div>
								)}
							</Show>
							<Show when={term.activeSubTasks > 0}>
								<div class={s.subRow} title={`${term.activeSubTasks} sub-tasks running`}>
									<SubTaskIcon />
									<span class={s.subText}>{term.activeSubTasks} sub-tasks running</span>
								</div>
							</Show>
							<Show when={term.agentIntent} keyed>
								{(intent) => (
									<div class={s.subRow} title={intent}>
										<IntentIcon />
										<span class={s.subText}>{truncate(intent)}</span>
									</div>
								)}
							</Show>
							{(() => {
								const prompt = term.lastPrompt;
								if (!prompt || term.agentIntent) return null;
								return (
									<div class={s.subRow} title={prompt}>
										<PromptIcon />
										<span class={s.subText}>{truncate(prompt)}</span>
									</div>
								);
							})()}
						</div>
					)}
				</For>
			</div>

			<div class={s.footer}>
				<span>{terminals().length} terminal(s)</span>
				<Show when={!props.embedded}>
					<span style={{ "margin-left": "auto" }}>↑↓ select • Return switch • 1-9 jump • Esc close</span>
				</Show>
			</div>
		</>
	);

	if (props.embedded) {
		return (
			<div class={s.dashboard} style={{ "max-height": "100vh", height: "100vh" }}>
				{dashboardContent()}
			</div>
		);
	}

	return (
		<Show when={isOpen()}>
			<div class={s.overlay} onClick={() => activityDashboardStore.close()}>
				<div class={s.dashboard} onClick={(e) => e.stopPropagation()}>
					{dashboardContent()}
				</div>
			</div>
		</Show>
	);
};
