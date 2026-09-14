import { type Component, createEffect, createMemo, createSignal, on, onCleanup, Show } from "solid-js";
import { useRepository } from "../../hooks/useRepository";
import { diffTabsStore } from "../../stores/diffTabs";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";
import { toastsStore } from "../../stores/toasts";
import { type DiffViewMode, uiStore } from "../../stores/ui";
import type { EditStep, FileReview, RevertResult, SessionReview, SessionSummary } from "../../types/sessionDiff";
import { cx } from "../../utils";
import { writeClipboard } from "../../utils/clipboard";
import { openFileAction } from "../../utils/filePreview";
import { ConfirmDialog } from "../ConfirmDialog";
import type { SearchOptions } from "../shared/DomSearchEngine";
import { DomSearchEngine } from "../shared/DomSearchEngine";
import { DomSearchOverview } from "../shared/DomSearchOverview";
import { createSearchVisibility, SearchBar } from "../shared/SearchBar";
import { buildRows, type SessionReviewMode } from "./buildRows";
import { SessionDiffList } from "./SessionDiffList";
import s from "./SessionDiffTab.module.css";
import { SessionPicker } from "./SessionPicker";

export interface SessionDiffTabProps {
	tabId: string;
	repoPath: string;
	sessionId?: string;
	onClose?: () => void;
}

type RevertTarget = { kind: "step"; step: EditStep } | { kind: "file"; group: FileReview };

const REVIEW_REFRESH_DEBOUNCE_MS = 2000;

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
	async function loadReview(sessionId: string) {
		setReviewError(null);
		if (!review()) setReviewLoading(true);
		const gen = ++reviewGen;
		try {
			const result = await repo.getSessionReview(props.repoPath, sessionId, includeSubagents());
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

	function selectSession(sessionId: string, persist = true) {
		setSelectedSessionId(sessionId);
		if (persist) diffTabsStore.setSessionId(props.tabId, sessionId);
		void loadReview(sessionId);
	}

	// Initial load.
	createEffect(() => {
		void loadSessions(false);
	});
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

	const rows = createMemo(() => {
		const r = review();
		if (!r) return [];
		return buildRows(r, viewMode(), expandedFiles(), openStepFiles());
	});

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
					onClick={() => setViewMode("file")}
					title="Group by file"
				>
					By file
				</button>
				<button
					type="button"
					class={cx(s.modeBtn, viewMode() === "chronological" && s.modeBtnActive)}
					onClick={() => setViewMode("chronological")}
					title="Flat chronological order"
				>
					Chronological
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
			</div>
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
						onOpenFile={handleOpenFile}
						onOpenAtLine={handleOpenAtLine}
						onRevertStep={requestRevertStep}
						onRevertFile={requestRevertFile}
						onCopyStep={handleCopyStep}
						onCopyFile={handleCopyFile}
						onToggleExpanded={toggleExpanded}
						onToggleStepsOpen={toggleStepsOpen}
						scrollRef={setScrollEl}
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
