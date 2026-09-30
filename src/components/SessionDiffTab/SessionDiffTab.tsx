import { type Component, createEffect, createMemo, createSignal, on, onCleanup, Show } from "solid-js";
import { useRepository } from "../../hooks/useRepository";
import { listen } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import { diffTabsStore } from "../../stores/diffTabs";
import { repositoriesStore } from "../../stores/repositories";
import { settingsStore } from "../../stores/settings";
import { terminalsStore } from "../../stores/terminals";
import { toastsStore } from "../../stores/toasts";
import { type DiffViewMode, uiStore } from "../../stores/ui";
import type { EditStep, FileReview, RevertResult, SessionReview, SessionSummary } from "../../types/sessionDiff";
import { cx } from "../../utils";
import { classifyFingerprintedDiff, classifyNewSteps } from "../../utils/classifyReviewDiff";
import { writeClipboard } from "../../utils/clipboard";
import { diffOptionsFromSettings } from "../../utils/diffOptionsFromSettings";
import { openFileAction } from "../../utils/filePreview";
import { navigateToTerminal } from "../../utils/navigateToTerminal";
import { ConfirmDialog } from "../ConfirmDialog";
import { DiffOptionsMenu } from "../shared/DiffOptionsMenu";
import type { SearchOptions } from "../shared/DomSearchEngine";
import { DomSearchEngine } from "../shared/DomSearchEngine";
import { DomSearchOverview } from "../shared/DomSearchOverview";
import type { DiffListNavHandle } from "../shared/diffListNav";
import { createSearchVisibility, SearchBar } from "../shared/SearchBar";
import { buildRows, rowIndexForStepIndex, type SessionReviewMode } from "./buildRows";
import { SessionDiffList } from "./SessionDiffList";
import s from "./SessionDiffTab.module.css";
import { SessionPicker } from "./SessionPicker";
import { TurnPicker } from "./TurnPicker";

export interface SessionDiffTabProps {
	tabId: string;
	repoPath: string;
	sessionId?: string;
	onClose?: () => void;
}

type RevertTarget = { kind: "step"; step: EditStep } | { kind: "file"; group: FileReview };

/** Fallback-only poll interval for a live session's working-tree-revision
 *  refresh. Now that `watchSessionReview`'s dedicated events
 *  (`session-review-changed`) are the primary live-update signal, this exists
 *  purely as a safety net for when the event transport is unavailable — slow
 *  enough that it's never the thing actually driving a normal live view. */
const REVIEW_REFRESH_DEBOUNCE_MS = 10_000;

/** Step-by-step review of everything one Claude Code session changed:
 *  grouped-by-file (default, with each file's cumulative diff and its
 *  individual edits collapsible underneath) or flat chronological order. */
export const SessionDiffTab: Component<SessionDiffTabProps> = (props) => {
	const repo = useRepository();

	const [sessions, setSessions] = createSignal<SessionSummary[]>([]);
	const [sessionsLoading, setSessionsLoading] = createSignal(false);
	const [selectedSessionId, setSelectedSessionId] = createSignal<string | null>(props.sessionId ?? null);

	const [review, setReview] = createSignal<SessionReview | null>(null);
	const [reviewLoading, setReviewLoading] = createSignal(false);
	const [reviewError, setReviewError] = createSignal<string | null>(null);
	const [warningsDismissed, setWarningsDismissed] = createSignal(false);

	const [viewMode, setViewMode] = createSignal<SessionReviewMode>("file");
	const [includeSubagents, setIncludeSubagents] = createSignal(true);
	const [expandedFiles, setExpandedFiles] = createSignal<Set<string>>(new Set());
	const [openStepFiles, setOpenStepFiles] = createSignal<Set<string>>(new Set());
	const [collapsedSteps, setCollapsedSteps] = createSignal<Set<string>>(new Set());
	const [listHandle, setListHandle] = createSignal<DiffListNavHandle | null>(null);

	// --- Live updates (session-review-changed / review-sessions-changed) ---
	/** Chronological mode only: auto-scroll to new steps as they arrive.
	 *  Defaults on — most reviewers watching a live session want to follow it. */
	const [followLive, setFollowLive] = createSignal(true);
	/** Chronological mode, Follow off: steps that arrived since the reviewer
	 *  last had them in view — powers the "N new changes" pill. Cleared
	 *  incrementally as the viewport scrolls past each one (see the effect
	 *  below), not all at once, so scrolling through them naturally drains it. */
	const [unseenStepIds, setUnseenStepIds] = createSignal<Set<string>>(new Set());
	/** File mode: abs_paths to flash-highlight right now (a visible file's
	 *  content just changed) — cleared per-key after the CSS fade via
	 *  `flashTimers`, not by a single blanket timeout, so a file that flashes
	 *  again mid-fade restarts its own timer instead of the two colliding. */
	const [flashKeys, setFlashKeys] = createSignal<Set<string>>(new Set());
	const flashTimers = new Map<string, ReturnType<typeof setTimeout>>();
	onCleanup(() => {
		for (const t of flashTimers.values()) clearTimeout(t);
		flashTimers.clear();
	});
	/** File mode: a live update touched a file that's currently off-screen —
	 *  held back rather than applied, so `review()` doesn't silently mutate
	 *  content the reviewer isn't looking at. `latestFetchedReview` is the
	 *  full fetch it came from, applied wholesale when the reviewer clicks
	 *  "Refresh". */
	const [pendingHiddenCount, setPendingHiddenCount] = createSignal(0);
	const [latestFetchedReview, setLatestFetchedReview] = createSignal<SessionReview | null>(null);
	/** File mode: a live update appended a new file below the current scroll
	 *  position. */
	const [hasNewFilesBelow, setHasNewFilesBelow] = createSignal(false);

	const [revertTarget, setRevertTarget] = createSignal<RevertTarget | null>(null);
	const [revertConfirmVisible, setRevertConfirmVisible] = createSignal(false);

	const {
		visible: searchVisible,
		focusToken: searchFocusToken,
		open: openSearchBar,
		close: closeSearchBar,
	} = createSearchVisibility();
	const [matchIndex, setMatchIndex] = createSignal(-1);
	const [matchCount, setMatchCount] = createSignal(0);
	const [overviewFractions, setOverviewFractions] = createSignal<number[]>([]);
	const [scrollEl, setScrollEl] = createSignal<HTMLElement>();
	let engine: DomSearchEngine | undefined;
	let lastSearchTerm = "";
	let lastSearchOpts: SearchOptions = { caseSensitive: false, regex: false, wholeWord: false };
	let searchDebounceTimer: ReturnType<typeof setTimeout> | undefined;

	createEffect(() => {
		diffTabsStore.setHandle(props.tabId, { openSearch: () => openSearchBar() });
		onCleanup(() => diffTabsStore.clearHandle(props.tabId));
	});

	const liveSessionId = () => terminalsStore.getActive()?.agentSessionId ?? null;
	const mode = (): DiffViewMode => (uiStore.state.diffViewMode === "split" ? "split" : "unified");
	const isLiveSession = () => selectedSessionId() !== null && selectedSessionId() === liveSessionId();

	async function loadSessions(preserveSelection: boolean) {
		setSessionsLoading(true);
		try {
			const list = await repo.listReviewSessions(props.repoPath, 20, true);
			setSessions(list);
			if (!preserveSelection || !selectedSessionId()) {
				const initial = props.sessionId ?? liveSessionId() ?? list[0]?.session_id ?? null;
				if (initial) selectSession(initial, false);
			}
		} finally {
			setSessionsLoading(false);
		}
	}

	let reviewGen = 0;
	/** Explicit (user-initiated) load: session switch, initial mount, the
	 *  subagent-filter toggle, a diff-options change, or a post-revert
	 *  refetch. Always replaces `review()` wholesale — unlike `refreshLiveReview`,
	 *  there is no "was this already on screen" question here, since the caller
	 *  itself just asked for this exact data. */
	async function loadReview(sessionId: string) {
		setReviewError(null);
		if (!review()) setReviewLoading(true);
		const gen = ++reviewGen;
		try {
			const result = await repo.getSessionReview(
				props.repoPath,
				sessionId,
				includeSubagents(),
				diffOptionsFromSettings(),
			);
			if (gen !== reviewGen) return;
			setReview(result);
			setWarningsDismissed(false);
		} catch (err) {
			if (gen !== reviewGen) return;
			setReviewError(String(err));
			setReview(null);
		} finally {
			if (gen === reviewGen) setReviewLoading(false);
		}
	}

	/** Row indices currently in the virtualizer's viewport, mapped to the file
	 *  they show — only meaningful in "file" mode, where a row IS a file. */
	function visibleFileAbsPaths(): Set<string> {
		const handle = listHandle();
		const result = new Set<string>();
		if (!handle) return result;
		const currentRows = rows();
		for (const idx of handle.visibleIndices()) {
			const row = currentRows[idx];
			if (row?.kind === "file") result.add(row.group.abs_path);
		}
		return result;
	}

	/** ~1.5s CSS fade (`.flash` in diffFileList.module.css) — per-key timers so a
	 *  file that changes again mid-fade restarts its own animation instead of
	 *  an earlier timer clearing it out from under the new one. */
	function flashFiles(absPaths: string[]) {
		if (absPaths.length === 0) return;
		setFlashKeys((prev) => {
			const next = new Set(prev);
			for (const p of absPaths) next.add(p);
			return next;
		});
		for (const p of absPaths) {
			const existing = flashTimers.get(p);
			if (existing) clearTimeout(existing);
			flashTimers.set(
				p,
				setTimeout(() => {
					flashTimers.delete(p);
					setFlashKeys((prev) => {
						if (!prev.has(p)) return prev;
						const next = new Set(prev);
						next.delete(p);
						return next;
					});
				}, 1500),
			);
		}
	}

	/** A rough "is the reviewer already near the bottom" heuristic — the nav
	 *  handle only exposes the topmost visible row, not a true bottom-of-range
	 *  index, so this treats "within 2 rows of the end" as "at the bottom"
	 *  rather than tracking the virtualizer's own end-of-range math a second
	 *  time here. */
	function nearBottom(): boolean {
		const handle = listHandle();
		if (!handle) return true;
		return handle.currentIndex() >= handle.rowCount() - 2;
	}

	/** Live update (from a `session-review-changed` event for the session
	 *  currently being viewed): unlike `loadReview`, this does NOT blindly
	 *  replace `review()`. Chronological mode always applies new steps (the
	 *  plan's "a change counts as seen once it's been in view" implies the
	 *  data IS there, just not necessarily scrolled to) and only differs on
	 *  whether it auto-scrolls (`followLive`) or raises the "N new changes"
	 *  pill. File mode applies new files and on-screen changes immediately
	 *  (flashing the latter), but holds an off-screen file's update behind
	 *  the "Refresh (N)" pill rather than mutating content the reviewer isn't
	 *  looking at. */
	async function refreshLiveReview() {
		const id = selectedSessionId();
		if (!id) return;
		const current = review();
		if (!current) return; // nothing on screen yet — the explicit load path owns this case
		let fresh: SessionReview;
		try {
			fresh = await repo.getSessionReview(props.repoPath, id, includeSubagents(), diffOptionsFromSettings());
		} catch (err) {
			// Live-update refresh failures are silent: the explicit loadReview
			// path (session switch, manual refresh) already surfaces errors for
			// user-initiated loads, and a transient failure here shouldn't blank
			// out perfectly good content already on screen.
			appLogger.error("git", "Failed to refresh a live session review", err);
			return;
		}
		if (fresh.session_id !== current.session_id) return; // session changed under us mid-fetch

		if (viewMode() === "chronological") {
			const newSteps = classifyNewSteps(current.steps, fresh.steps);
			setReview(fresh);
			if (newSteps.length === 0) return;
			if (followLive()) {
				queueMicrotask(() => listHandle()?.scrollToIndex(rows().length - 1, { align: "end" }));
			} else {
				setUnseenStepIds((prev) => {
					const next = new Set(prev);
					for (const step of newSteps) next.add(step.tool_use_id);
					return next;
				});
			}
			return;
		}

		// "file" mode
		const visible = visibleFileAbsPaths();
		const { newKeys, changedVisible, changedHidden } = classifyFingerprintedDiff(
			current.files.map((f) => ({ key: f.abs_path, fingerprint: f.revision })),
			fresh.files.map((f) => ({ key: f.abs_path, fingerprint: f.revision })),
			visible,
		);

		if (changedHidden.length === 0) {
			setReview(fresh);
			setPendingHiddenCount(0);
			setLatestFetchedReview(null);
		} else {
			// Hybrid: fresh's files, except a changedHidden entry keeps the
			// CURRENT (old) FileReview object, so it doesn't visually change.
			const oldByPath = new Map(current.files.map((f) => [f.abs_path, f]));
			const hiddenSet = new Set(changedHidden);
			const hybridFiles = fresh.files.map((f) => (hiddenSet.has(f.abs_path) ? (oldByPath.get(f.abs_path) ?? f) : f));
			setReview({ ...fresh, files: hybridFiles });
			setPendingHiddenCount(changedHidden.length);
			setLatestFetchedReview(fresh);
		}
		if (changedVisible.length > 0) flashFiles(changedVisible);
		if (newKeys.length > 0 && !nearBottom()) setHasNewFilesBelow(true);
	}

	/** "Refresh (N)" pill: apply every held-back off-screen update at once. */
	function applyPendingHiddenUpdates() {
		const fresh = latestFetchedReview();
		if (!fresh) return;
		setReview(fresh);
		setPendingHiddenCount(0);
		setLatestFetchedReview(null);
	}

	/** "New content below" pill (file mode). */
	function jumpToNewContentBelow() {
		listHandle()?.scrollToIndex(Math.max(rows().length - 1, 0), { align: "end" });
		setHasNewFilesBelow(false);
	}

	/** "N new changes" pill (chronological mode, Follow off): jumps to the
	 *  earliest still-unseen step. */
	function jumpToFirstUnseenStep() {
		const ids = unseenStepIds();
		if (ids.size === 0) return;
		const r = review();
		if (!r) return;
		let minStepIndex: number | null = null;
		for (const step of r.steps) {
			if (ids.has(step.tool_use_id) && (minStepIndex === null || step.step_index < minStepIndex)) {
				minStepIndex = step.step_index;
			}
		}
		if (minStepIndex !== null) handleJumpToStep(minStepIndex);
	}

	// As the viewport scrolls down, drop any unseen step whose row has already
	// scrolled past the top — it's been "in view" per the plan's own wording
	// for when a change counts as seen. Strict less-than (not <=) so the row
	// exactly at the top, which may only just now have appeared, isn't marked
	// seen before the reviewer could plausibly have looked at it.
	createEffect(() => {
		const handle = listHandle();
		if (!handle || viewMode() !== "chronological") return;
		const top = handle.currentIndex();
		const ids = unseenStepIds();
		if (ids.size === 0) return;
		const currentRows = rows();
		const rowIndexByToolUseId = new Map<string, number>();
		for (let i = 0; i < currentRows.length; i++) {
			const row = currentRows[i];
			if (row.kind === "step") rowIndexByToolUseId.set(row.step.tool_use_id, i);
		}
		let changed = false;
		const next = new Set(ids);
		for (const id of ids) {
			const rowIdx = rowIndexByToolUseId.get(id);
			if (rowIdx !== undefined && rowIdx < top) {
				next.delete(id);
				changed = true;
			}
		}
		if (changed) setUnseenStepIds(next);
	});

	function selectSession(sessionId: string, persist = true) {
		if (sessionId !== selectedSessionId()) {
			// Clear the previous session's content synchronously — otherwise it
			// stays on screen (stale) until the new session's review resolves,
			// since `loadReview`'s loading state only shows when `!review()`.
			setReview(null);
			setReviewError(null);
			setOpenStepFiles(new Set<string>());
			setCollapsedSteps(new Set<string>());
			setWarningsDismissed(false);
			scrollEl()?.scrollTo({ top: 0 });
			// A new session starts with no live-update backlog of its own.
			setUnseenStepIds(new Set<string>());
			setFlashKeys(new Set<string>());
			setPendingHiddenCount(0);
			setLatestFetchedReview(null);
			setHasNewFilesBelow(false);
		}
		setSelectedSessionId(sessionId);
		if (persist) diffTabsStore.setSessionId(props.tabId, sessionId);
		void loadReview(sessionId);
	}

	// Initial load.
	createEffect(() => {
		void loadSessions(false);
	});
	// An already-open tab can be re-targeted at a different session (e.g. a
	// second `addSessionReview` call for the same repo/tab) — `sessionId` was
	// previously read only once, at signal-init time, so the tab never
	// followed a later change. `{defer: true}` skips the redundant fire at
	// mount, which `loadSessions`' own initial-selection logic already covers.
	createEffect(
		on(
			() => props.sessionId,
			(id) => {
				if (id && id !== selectedSessionId()) selectSession(id);
			},
			{ defer: true },
		),
	);
	// Reloading the review when the subagent filter changes — `{ defer: true }`
	// so this doesn't ALSO fire on first mount (selectSession()'s own initial
	// load already covers that; without `defer` this ran a redundant second
	// getSessionReview on every mount).
	createEffect(
		on(
			includeSubagents,
			() => {
				const id = selectedSessionId();
				if (id) void loadReview(id);
			},
			{ defer: true },
		),
	);

	// Live sessions re-fetch on a working-tree revision bump (debounced); a
	// static (past) session never re-fetches on its own — matching this
	// codebase's getRevision-vs-getGitRevision panel convention: a surface
	// showing historical data has no business re-running backend work on
	// every file save.
	let liveDebounce: ReturnType<typeof setTimeout> | undefined;
	createEffect(() => {
		void repositoriesStore.getRevision(props.repoPath);
		if (!isLiveSession()) return;
		clearTimeout(liveDebounce);
		liveDebounce = setTimeout(() => {
			const id = selectedSessionId();
			if (id) void loadReview(id);
		}, REVIEW_REFRESH_DEBOUNCE_MS);
		onCleanup(() => clearTimeout(liveDebounce));
	});

	// Starts (or moves) a backend watch on the selected session's transcript —
	// the source of the `session-review-changed`/`agent-edit-observed` events
	// this tab and `useAppInit.ts`'s app-level listeners both react to.
	// Ref-counted on the backend, so overlapping subscribers (this tab plus a
	// second one open on the same session) are safe.
	let watchedSessionId: string | null = null;
	createEffect(() => {
		const id = selectedSessionId();
		if (id === watchedSessionId) return;
		const prevId = watchedSessionId;
		watchedSessionId = id;
		if (prevId) void repo.unwatchSessionReview(props.repoPath, prevId);
		if (id) void repo.watchSessionReview(props.repoPath, id);
	});
	onCleanup(() => {
		if (watchedSessionId) void repo.unwatchSessionReview(props.repoPath, watchedSessionId);
	});

	// Primary live-update signal: a dedicated event for the exact session
	// being watched, instead of the coarse revision-bump poll above. Reads
	// `selectedSessionId()` at EVENT time (not as a tracked effect dependency),
	// so this subscription is set up once per mount, not re-registered on
	// every session switch.
	createEffect(() => {
		let unlisten: (() => void) | undefined;
		listen<{ repo_path: string; session_id: string }>("session-review-changed", (event) => {
			const { repo_path, session_id } = event.payload;
			if (repo_path === props.repoPath && session_id === selectedSessionId()) void refreshLiveReview();
		})
			.then((fn) => {
				unlisten = fn;
			})
			.catch((err) => appLogger.error("app", "Failed to register session-review-changed listener", err));
		onCleanup(() => unlisten?.());
	});

	// The session dropdown refreshes itself as new sessions appear or existing
	// ones grow enough to change their summary (last prompt, counts) — not
	// only when the picker is opened by hand.
	createEffect(() => {
		let unlisten: (() => void) | undefined;
		listen<{ repo_path: string }>("review-sessions-changed", (event) => {
			if (event.payload.repo_path === props.repoPath) void loadSessions(true);
		})
			.then((fn) => {
				unlisten = fn;
			})
			.catch((err) => appLogger.error("app", "Failed to register review-sessions-changed listener", err));
		onCleanup(() => unlisten?.());
	});

	// A whitespace/case diff-option change re-fetches with the new options —
	// `{defer: true}` skips the redundant fire at mount (the initial
	// `selectSession` load already reads the current options).
	createEffect(
		on(
			() => [
				settingsStore.state.diffIgnoreLeadingWhitespace,
				settingsStore.state.diffIgnoreTrailingWhitespace,
				settingsStore.state.diffIgnoreWhitespaceAmount,
				settingsStore.state.diffIgnoreCase,
			],
			() => {
				const id = selectedSessionId();
				if (id) void loadReview(id);
			},
			{ defer: true },
		),
	);

	const rows = createMemo(() => {
		const r = review();
		if (!r) return [];
		return buildRows(r, viewMode(), expandedFiles(), openStepFiles(), collapsedSteps());
	});

	function changeViewMode(next: SessionReviewMode) {
		if (next === viewMode()) return;
		setViewMode(next);
		// Switching mode swaps every row's kind (file rows <-> step rows) — the
		// old scroll offset means nothing against the new row set.
		scrollEl()?.scrollTo({ top: 0 });
	}

	function toggleExpanded(absPath: string) {
		setExpandedFiles((prev) => {
			const next = new Set(prev);
			if (next.has(absPath)) next.delete(absPath);
			else next.add(absPath);
			return next;
		});
	}
	function toggleStepsOpen(absPath: string) {
		setOpenStepFiles((prev) => {
			const next = new Set(prev);
			if (next.has(absPath)) next.delete(absPath);
			else next.add(absPath);
			return next;
		});
	}
	function toggleStepCollapsed(toolUseId: string) {
		setCollapsedSteps((prev) => {
			const next = new Set(prev);
			if (next.has(toolUseId)) next.delete(toolUseId);
			else next.add(toolUseId);
			return next;
		});
	}

	/** Jumps to a specific step by its (backend-assigned, snapshot-stable)
	 *  `step_index` — used by both the `^`/`v` same-file buttons and the turn
	 *  picker. Only meaningful in chronological mode, where rows are steps
	 *  1:1; `rowIndexForStepIndex` returns `null` in by-file mode, where
	 *  there's no single row that "is" a given step. */
	function handleJumpToStep(stepIndex: number) {
		const idx = rowIndexForStepIndex(rows(), stepIndex);
		if (idx !== null) listHandle()?.scrollToIndex(idx, { align: "center" });
	}

	/** `<`/`>` toolbar buttons — step to the adjacent ROW, which is a step in
	 *  chronological mode and a file in by-file mode; the list's own nav
	 *  handle doesn't need to know which. */
	function goToAdjacentRow(delta: number) {
		const handle = listHandle();
		const total = rows().length;
		if (!handle || total === 0) return;
		const next = Math.min(Math.max(handle.currentIndex() + delta, 0), total - 1);
		handle.scrollToIndex(next, { align: "center" });
	}

	// Default every file to expanded the first time a session's review
	// loads, so the reviewer doesn't have to click through every row to see
	// anything. Keyed on the review's own session_id (not merely "review()
	// changed") and merges rather than replaces on any later update for the
	// *same* session — a live session's ~2s debounced poll refresh (and a
	// post-revert refetch) also update `review()`, and unconditionally
	// resetting on every update silently re-expanded every file the user
	// had just collapsed moments earlier.
	let expandedDefaultsForSession: string | null = null;
	let knownFileIds = new Set<string>();
	createEffect(() => {
		const r = review();
		if (!r) return;
		const ids = r.files.map((f) => f.abs_path);
		if (expandedDefaultsForSession !== r.session_id) {
			expandedDefaultsForSession = r.session_id;
			knownFileIds = new Set(ids);
			setExpandedFiles(new Set(ids));
			return;
		}
		// Same session, a later refresh (live poll or post-revert refetch):
		// only auto-expand files we haven't seen before — never re-expand one
		// the user has since collapsed just because it's still present.
		const newlyAppeared = ids.filter((id) => !knownFileIds.has(id));
		knownFileIds = new Set(ids);
		if (newlyAppeared.length === 0) return;
		setExpandedFiles((prev) => {
			const next = new Set(prev);
			for (const id of newlyAppeared) next.add(id);
			return next;
		});
	});

	// A subagent step has no PTY of its own — it jumps to the PARENT session's
	// tab, so this is resolved once from `review().tuic_session_id`, not
	// derived per-step. A stable function reference (never recreated) so the
	// `<Show>`-gated badge below never goes stale across a truthy->truthy
	// value change — only the undefined<->function transition ever happens.
	const agentTabId = createMemo(() => {
		const tuicSessionId = review()?.tuic_session_id;
		return tuicSessionId ? terminalsStore.getTerminalForSession(tuicSessionId) : null;
	});
	function handleJumpToAgent() {
		const id = agentTabId();
		if (id) navigateToTerminal(id);
	}

	function handleOpenFile(absPath: string) {
		const group = review()?.files.find((f) => f.abs_path === absPath);
		openFileAction(group?.rel_path ?? absPath, props.repoPath);
	}

	function handleOpenAtLine(step: EditStep, line: number) {
		openFileAction(step.rel_path ?? step.abs_path, props.repoPath, undefined, line);
	}

	async function handleCopyStep(step: EditStep) {
		await writeClipboard(step.patch);
		toastsStore.add("Copied", `${step.rel_path ?? step.abs_path}'s diff copied to clipboard`, "info");
	}

	async function handleCopyFile(group: FileReview) {
		await writeClipboard(group.cumulative_patch);
		toastsStore.add("Copied", `${group.display_path}'s diff copied to clipboard`, "info");
	}

	async function handleCopyAll() {
		const r = review();
		if (!r) return;
		const combined = r.files
			.map((f) => f.cumulative_patch)
			.filter(Boolean)
			.join("\n");
		await writeClipboard(combined);
		toastsStore.add("Copied", "This session's combined diff copied to clipboard", "info");
	}

	function requestRevertStep(step: EditStep) {
		setRevertTarget({ kind: "step", step });
		setRevertConfirmVisible(true);
	}
	function requestRevertFile(group: FileReview) {
		setRevertTarget({ kind: "file", group });
		setRevertConfirmVisible(true);
	}
	function cancelRevert() {
		setRevertConfirmVisible(false);
		setRevertTarget(null);
	}

	function methodLabel(method: RevertResult["method"]): string {
		switch (method) {
			case "git_apply_reverse":
				return "Reversed via patch";
			case "string_substitution":
				return "Applied string substitution";
			case "restore_backup":
				return "Restored from session-start backup";
			case "write_base":
				return "Restored to reconstructed session-start content";
			case "delete_file":
				return "File deleted (created this session)";
		}
	}

	async function confirmRevert() {
		const target = revertTarget();
		setRevertConfirmVisible(false);
		setRevertTarget(null);
		const sessionId = selectedSessionId();
		if (!target || !sessionId) return;

		try {
			if (target.kind === "step") {
				const result = await repo.revertSessionStep(props.repoPath, sessionId, target.step.tool_use_id);
				reportRevertResult(result);
			} else {
				await performFileRevert(sessionId, target.group, false);
			}
		} catch (err) {
			toastsStore.add("Revert failed", String(err), "error");
		}
	}

	async function performFileRevert(sessionId: string, group: FileReview, force: boolean) {
		const result = await repo.revertFileToSessionStart(props.repoPath, sessionId, group.abs_path, force);
		if (!result.applied && !force && result.message?.toLowerCase().includes("force")) {
			toastsStore.add("Can't revert — file changed since", result.message ?? "", "warn", false, {
				label: "Force revert anyway",
				onClick: () => void performFileRevert(sessionId, group, true),
			});
			return;
		}
		reportRevertResult(result);
	}

	function reportRevertResult(result: RevertResult) {
		if (result.applied) {
			toastsStore.add("Reverted", methodLabel(result.method), "info");
			const id = selectedSessionId();
			if (id) void loadReview(id);
		} else {
			toastsStore.add("Nothing reverted", result.message ?? "Could not revert", "warn");
		}
	}

	function rerunSearch() {
		const el = scrollEl();
		if (!el) return;
		engine = new DomSearchEngine(el);
		const count = engine.search(lastSearchTerm, lastSearchOpts);
		setMatchCount(count);
		setMatchIndex(count > 0 ? 0 : -1);
		setOverviewFractions(count > 0 ? engine.matchFractions(el) : []);
	}
	const handleSearch = (term: string, opts: SearchOptions) => {
		lastSearchTerm = term;
		lastSearchOpts = opts;
		clearTimeout(searchDebounceTimer);
		if (!term) {
			engine?.clear();
			setMatchCount(0);
			setMatchIndex(-1);
			setOverviewFractions([]);
			return;
		}
		searchDebounceTimer = setTimeout(rerunSearch, 150);
	};
	const handleSearchNext = () => {
		if (!engine || matchCount() === 0) return;
		setMatchIndex(engine.next());
	};
	const handleSearchPrev = () => {
		if (!engine || matchCount() === 0) return;
		setMatchIndex(engine.prev());
	};
	const handleSearchClose = () => {
		engine?.clear();
		closeSearchBar();
		setMatchCount(0);
		setMatchIndex(-1);
		setOverviewFractions([]);
	};

	const confirmTitle = () => {
		const t = revertTarget();
		if (!t) return "";
		return t.kind === "step"
			? `Revert this step in ${t.step.rel_path ?? t.step.abs_path}?`
			: `Revert ${t.group.display_path} to session start?`;
	};
	const confirmMessage = () => {
		const t = revertTarget();
		if (!t) return "";
		if (t.kind === "step") {
			return "Undoes just this edit, keeping every later edit. Fails if a later edit changed the same region.";
		}
		return `Undoes all ${t.group.step_indices.length} edit(s) to this file, restoring it to what it looked like before this session.`;
	};

	return (
		<div class={s.content}>
			<div class={s.toolbar}>
				<SessionPicker
					sessions={sessions()}
					selectedId={selectedSessionId()}
					liveId={liveSessionId()}
					loading={sessionsLoading()}
					onSelect={selectSession}
					onRefresh={() => void loadSessions(true)}
				/>
				<div class={s.toolbarSpacer} />
				<button
					type="button"
					class={cx(s.modeBtn, viewMode() === "file" && s.modeBtnActive)}
					onClick={() => changeViewMode("file")}
					title="Group by file"
				>
					By file
				</button>
				<button
					type="button"
					class={cx(s.modeBtn, viewMode() === "chronological" && s.modeBtnActive)}
					onClick={() => changeViewMode("chronological")}
					title="Flat chronological order"
				>
					Chronological
				</button>
				<Show when={viewMode() === "chronological"}>
					<TurnPicker turns={review()?.turns ?? []} steps={review()?.steps ?? []} onSelect={handleJumpToStep} />
					<label class={s.checkboxLabel} title="Auto-scroll to new changes as they arrive">
						<input type="checkbox" checked={followLive()} onChange={(e) => setFollowLive(e.currentTarget.checked)} />
						Follow
					</label>
				</Show>
				<button
					type="button"
					class={s.iconBtn}
					onClick={() => goToAdjacentRow(-1)}
					disabled={rows().length === 0 || (listHandle()?.currentIndex() ?? 0) <= 0}
					title={viewMode() === "chronological" ? "Previous change" : "Previous file"}
				>
					<svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor">
						<path d="M10 3l-5 5 5 5" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
					</svg>
				</button>
				<button
					type="button"
					class={s.iconBtn}
					onClick={() => goToAdjacentRow(1)}
					disabled={rows().length === 0 || (listHandle()?.currentIndex() ?? 0) >= rows().length - 1}
					title={viewMode() === "chronological" ? "Next change" : "Next file"}
				>
					<svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor">
						<path d="M6 3l5 5-5 5" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
					</svg>
				</button>
				<label class={s.checkboxLabel}>
					<input
						type="checkbox"
						checked={includeSubagents()}
						onChange={(e) => setIncludeSubagents(e.currentTarget.checked)}
					/>
					Subagent edits
				</label>
				<button type="button" class={s.modeBtn} onClick={handleCopyAll} title="Copy the whole session's combined diff">
					Copy all
				</button>
				<DiffOptionsMenu />
			</div>
			<Show when={viewMode() === "chronological" && !followLive() && unseenStepIds().size > 0}>
				<div class={s.liveUpdateBar}>
					<button type="button" class={s.pillBtn} onClick={jumpToFirstUnseenStep}>
						{unseenStepIds().size} new change{unseenStepIds().size > 1 ? "s" : ""}
					</button>
				</div>
			</Show>
			<Show when={viewMode() === "file" && (hasNewFilesBelow() || pendingHiddenCount() > 0)}>
				<div class={s.liveUpdateBar}>
					<Show when={hasNewFilesBelow()}>
						<button type="button" class={s.pillBtn} onClick={jumpToNewContentBelow}>
							New content below
						</button>
					</Show>
					<Show when={pendingHiddenCount() > 0}>
						<button type="button" class={s.pillBtn} onClick={applyPendingHiddenUpdates}>
							Refresh ({pendingHiddenCount()})
						</button>
					</Show>
				</div>
			</Show>
			<SearchBar
				visible={searchVisible()}
				focusToken={searchFocusToken()}
				onSearch={handleSearch}
				onNext={handleSearchNext}
				onPrev={handleSearchPrev}
				onClose={handleSearchClose}
				matchIndex={matchIndex()}
				matchCount={matchCount()}
			/>
			<Show when={review()?.warnings.length && !warningsDismissed()}>
				<div class={s.warningsBanner}>
					<span>
						{review()?.warnings.length} issue{(review()?.warnings.length ?? 0) > 1 ? "s" : ""} while reading this
						session's transcript — some steps may be incomplete.
					</span>
					<button type="button" class={s.linkBtn} onClick={() => setWarningsDismissed(true)}>
						Dismiss
					</button>
				</div>
			</Show>
			<div class={s.body}>
				<Show when={reviewLoading()}>
					<div class={s.emptyState}>Loading session…</div>
				</Show>
				<Show when={!reviewLoading() && reviewError()}>
					<div class={s.emptyState}>Error: {reviewError()}</div>
				</Show>
				<Show when={!reviewLoading() && !reviewError() && review() && rows().length === 0}>
					<div class={s.emptyState}>This session made no file edits.</div>
				</Show>
				<Show when={!reviewLoading() && !reviewError() && rows().length > 0}>
					<SessionDiffList
						rows={rows()}
						mode={mode()}
						wrap={uiStore.state.diffSoftWrap}
						maxLines={settingsStore.state.sessionDiffTruncateLines}
						flashKeys={flashKeys()}
						onOpenFile={handleOpenFile}
						onOpenAtLine={handleOpenAtLine}
						onRevertStep={requestRevertStep}
						onRevertFile={requestRevertFile}
						onCopyStep={handleCopyStep}
						onCopyFile={handleCopyFile}
						onToggleExpanded={toggleExpanded}
						onToggleStepsOpen={toggleStepsOpen}
						onToggleStepCollapsed={toggleStepCollapsed}
						onJumpToStep={handleJumpToStep}
						onJumpToAgent={agentTabId() ? handleJumpToAgent : undefined}
						scrollRef={setScrollEl}
						ref={setListHandle}
					/>
				</Show>
				<Show when={searchVisible()}>
					<DomSearchOverview scrollEl={scrollEl} fractions={overviewFractions} />
				</Show>
			</div>
			<ConfirmDialog
				visible={revertConfirmVisible()}
				title={confirmTitle()}
				message={confirmMessage()}
				confirmLabel="Revert"
				kind="warning"
				defaultButton="cancel"
				onConfirm={confirmRevert}
				onClose={cancelRevert}
			/>
		</div>
	);
};
