import { type Component, createEffect, createMemo, createSignal, For, onCleanup, Show } from "solid-js";
import { usePty } from "../../hooks/usePty";
import { useRepository } from "../../hooks/useRepository";
import { t } from "../../i18n";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import { diffTabsStore } from "../../stores/diffTabs";
import { editorTabsStore } from "../../stores/editorTabs";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";
import { type DiffViewMode, uiStore } from "../../stores/ui";
import { cx } from "../../utils";
import { ConfirmDialog } from "../ConfirmDialog";
import type { SearchOptions } from "../shared/DomSearchEngine";
import { DomSearchEngine } from "../shared/DomSearchEngine";
import { DomSearchOverview } from "../shared/DomSearchOverview";
import { createSearchVisibility, SearchBar } from "../shared/SearchBar";
import { DiffViewer } from "../ui/DiffViewer";
import { BranchDiffScrollView } from "./BranchDiffScrollView";
import { CommentBox } from "./CommentBox";
import s from "./DiffTab.module.css";
import { buildPartialPatch, extractHunks, extractSelectedLines } from "./diffPatch";
import { diffLineCount, isDiffTooLarge } from "./diffSize";
import { sendDiffComment } from "./sendDiffComment";
import { createLineSelection } from "./useLineSelection";

export interface DiffTabProps {
	tabId: string;
	repoPath: string;
	filePath: string;
	scope?: string;
	untracked?: boolean;
	onClose?: () => void;
}

/** Check if this diff tab supports restore actions (working tree or staged, not commit/untracked) */
function canRestore(scope?: string, untracked?: boolean): boolean {
	if (untracked) return false;
	// scope is undefined (working tree) or "staged" — both support restore
	// scope is a commit hash — read-only
	if (!scope || scope === "staged") return true;
	return false;
}

export const DiffTab: Component<DiffTabProps> = (props) => {
	const pty = usePty();
	const [diff, setDiff] = createSignal("");
	const [loading, setLoading] = createSignal(false);
	const [error, setError] = createSignal<string | null>(null);
	const repo = useRepository();

	// Search state
	const {
		visible: searchVisible,
		focusToken: searchFocusToken,
		open: openSearchBar,
		close: closeSearchBar,
	} = createSearchVisibility();
	const [matchIndex, setMatchIndex] = createSignal(-1);
	const [matchCount, setMatchCount] = createSignal(0);
	/** Match-center fractions for the scrollbar overview ticks. */
	const [overviewFractions, setOverviewFractions] = createSignal<number[]>([]);
	/** The overflow scroll container (the diff wrapper) the matches live in. */
	const [scrollEl, setScrollEl] = createSignal<HTMLElement>();

	// Hunk restore state
	const [confirmVisible, setConfirmVisible] = createSignal(false);
	const [pendingHunkPatch, setPendingHunkPatch] = createSignal<string | null>(null);
	const [hoverHunkIdx, setHoverHunkIdx] = createSignal<number | null>(null);

	/** Parsed hunks, memoized — re-parsing the whole diff per revert/select op was wasteful */
	const hunks = createMemo(() => extractHunks(diff()));

	// Line-level drag selection — shared with SessionDiffTab.
	const lineSelection = createLineSelection({
		hunks,
		selectedClass: s.lineSelected,
		ignoreSelector: `.${s.revertBtn}`,
	});
	const selectedLines = lineSelection.selectedLines;
	const selectedHunkIdx = lineSelection.selectedHunkIdx;

	// Pathological single-file diffs (full-file rewrites) would render thousands
	// of DOM rows — @git-diff-view has no virtualization. Guard above this line
	// count and let the user opt in. (perf pass 2026-06-07)
	const diffLines = createMemo(() => diffLineCount(diff()));
	const tooLarge = createMemo(() => isDiffTooLarge(diff()));
	const [forceRenderLarge, setForceRenderLarge] = createSignal(false);

	// Clear selection when diff changes (and drop the DOM row-mapping cache —
	// the DiffViewer rebuilds its rows on diff/mode change)
	createEffect(() => {
		diff();
		uiStore.state.diffViewMode;
		lineSelection.clear();
		setForceRenderLarge(false);
		lineSelection.invalidate();
	});

	let contentRef: HTMLElement | undefined;

	let engine: DomSearchEngine | undefined;
	let lastSearchTerm = "";
	let lastSearchOpts: SearchOptions = { caseSensitive: false, regex: false, wholeWord: false };
	let searchDebounceTimer: ReturnType<typeof setTimeout> | undefined;

	// Expose openSearch for Cmd+F
	createEffect(() => {
		diffTabsStore.setHandle(props.tabId, { openSearch: () => openSearchBar() });
		onCleanup(() => diffTabsStore.clearHandle(props.tabId));
	});

	// Load file diff when props change or the repo revision bumps (git index/HEAD changed)
	let diffGen = 0;
	createEffect(() => {
		const repoPath = props.repoPath;
		const filePath = props.filePath;
		const scope = props.scope;
		void (repoPath ? repositoriesStore.getRevision(repoPath) : 0);

		if (!repoPath || !filePath) {
			setDiff("");
			return;
		}

		if (!diff()) setLoading(true);
		setError(null);

		// A revision-bump burst can fire several loads; only the newest may settle
		// state, so stale awaits don't overwrite the current diff.
		const gen = ++diffGen;
		(async () => {
			try {
				const diffContent = await repo.getFileDiff(repoPath, filePath, scope, props.untracked);
				if (gen !== diffGen) return;
				setDiff(diffContent);
			} catch (err) {
				if (gen !== diffGen) return;
				setError(String(err));
				setDiff("");
			} finally {
				if (gen === diffGen) setLoading(false);
			}
		})();
	});

	// Re-apply search when diff content changes or view mode changes
	createEffect(() => {
		diff();
		uiStore.state.diffViewMode;
		if (!searchVisible() || !lastSearchTerm) return;
		// Cancel a still-pending RAF on re-run and on unmount (Solid runs this
		// onCleanup before each re-execution and on disposal).
		const raf = requestAnimationFrame(() => rerunSearch());
		onCleanup(() => cancelAnimationFrame(raf));
	});

	function rerunSearch() {
		if (!contentRef) return;
		engine = new DomSearchEngine(contentRef);
		const count = engine.search(lastSearchTerm, lastSearchOpts);
		setMatchCount(count);
		setMatchIndex(count > 0 ? 0 : -1);
		const el = scrollEl();
		setOverviewFractions(el && count > 0 ? engine.matchFractions(el) : []);
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

		searchDebounceTimer = setTimeout(() => {
			rerunSearch();
		}, 150);
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

	/** One-sided diffs (new/deleted files) only support unified view */
	const isOneSided = (): boolean => {
		const d = diff();
		return !!d && (d.includes("new file mode") || d.includes("deleted file mode"));
	};

	/** Force unified mode for one-sided diffs where split wastes half the screen */
	const mode = (): DiffViewMode => {
		if (isOneSided()) return "unified";
		return uiStore.state.diffViewMode;
	};

	// --- Hunk restore ---

	/** Find the hunk index from a DOM element within the diff content */
	function findHunkIndex(el: HTMLElement): number | null {
		// Walk up to find a .diff-line-hunk-action or .diff-line-hunk-content
		const hunkRow = el.closest("tr, [class*='diff-line-hunk']")?.closest("tr");
		if (!hunkRow || !contentRef) return null;
		// Count how many hunk rows precede this one in the DOM
		const allHunkRows = contentRef.querySelectorAll("[class*='diff-line-hunk-content']");
		for (let i = 0; i < allHunkRows.length; i++) {
			if (allHunkRows[i].closest("tr") === hunkRow) return i;
		}
		return null;
	}

	function handleRevertClick(hunkIdx: number) {
		const h = hunks();
		if (hunkIdx < 0 || hunkIdx >= h.length) return;
		setPendingHunkPatch(h[hunkIdx]);
		setConfirmVisible(true);
	}

	async function confirmRevert() {
		const patch = pendingHunkPatch();
		if (!patch) return;
		setConfirmVisible(false);
		setPendingHunkPatch(null);

		try {
			await invoke("git_apply_reverse_patch", {
				path: props.repoPath,
				patch,
				scope: props.scope || undefined,
			});
		} catch (err) {
			appLogger.error("git", "Failed to revert hunk", err);
		}
		lineSelection.clear();
		// The repo revision bump from git changes will trigger diff reload automatically
	}

	function cancelRevert() {
		setConfirmVisible(false);
		setPendingHunkPatch(null);
	}

	const isStaged = () => props.scope === "staged";

	function handleRestoreSelected() {
		const hIdx = selectedHunkIdx();
		if (hIdx === null) return;
		const sel = selectedLines();
		if (sel.size === 0) return;

		const patch = buildPartialPatch(diff(), hIdx, sel);
		if (!patch) return;

		setPendingHunkPatch(patch);
		setConfirmVisible(true);
	}

	// --- Comment on selected lines ---
	const [commentVisible, setCommentVisible] = createSignal(false);
	const [commentText, setCommentText] = createSignal("");
	const [commentError, setCommentError] = createSignal<string | null>(null);
	let commentTextareaRef: HTMLTextAreaElement | undefined;

	function handleCommentOpen() {
		setCommentVisible(true);
		setCommentError(null);
		requestAnimationFrame(() => commentTextareaRef?.focus());
	}

	async function handleCommentSend() {
		const hIdx = selectedHunkIdx();
		if (hIdx === null) return;

		const { lines, startLine, endLine } = extractSelectedLines(diff(), hIdx, selectedLines());
		const term = terminalsStore.findTerminalWithSession();

		const result = await sendDiffComment(
			{ filePath: props.filePath, startLine, endLine, lines },
			commentText(),
			term?.sessionId,
			term?.agentType,
			(sessionId, message, agentType) => pty.sendCommand(sessionId, message, agentType),
		);

		if (result === "ok") {
			setCommentText("");
			setCommentError(null);
			setCommentVisible(false);
			lineSelection.clear();
		} else if (result === "no-terminal") {
			setCommentError("No terminal with active session — open a terminal first");
		} else if (result === "error") {
			appLogger.error("git", "Failed to send comment to terminal");
			setCommentError("Failed to send — see logs for details");
		}
	}

	const selectedCount = () => selectedLines().size;

	const confirmTitle = () => {
		if (selectedCount() > 0) {
			const count = selectedCount();
			return isStaged()
				? `Unstage ${count} selected line${count > 1 ? "s" : ""}?`
				: `Discard ${count} selected line${count > 1 ? "s" : ""}?`;
		}
		return isStaged()
			? t("diffTab.unstageHunkTitle", "Unstage this change?")
			: t("diffTab.discardHunkTitle", "Discard this change?");
	};
	const confirmMessage = () =>
		isStaged()
			? t(
					"diffTab.unstageHunkMsg",
					"This will remove this hunk from the staging area. The changes will remain in your working directory.",
				)
			: t("diffTab.discardHunkMsg", "This will permanently discard this change. This cannot be undone.");

	return (
		<div class={s.content}>
			{/* Toolbar with view mode toggle */}
			<div class={s.toolbar}>
				<button
					class={cx(s.modeBtn, mode() === "split" && s.modeBtnActive)}
					onClick={() => uiStore.setDiffViewMode("split")}
					title={t("diffTab.splitView", "Side-by-side")}
					disabled={isOneSided()}
				>
					<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
						<path d="M1 2h6v12H1V2zm8 0h6v12H9V2zM2 3v10h4V3H2zm8 0v10h4V3h-4z" />
					</svg>
				</button>
				<button
					class={cx(s.modeBtn, mode() === "unified" && s.modeBtnActive)}
					onClick={() => uiStore.setDiffViewMode("unified")}
					title={t("diffTab.unifiedView", "Inline")}
				>
					<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
						<path d="M1 2h14v12H1V2zm1 1v10h12V3H2z" />
					</svg>
				</button>
				<button
					class={cx(s.modeBtn, mode() === "scroll" && s.modeBtnActive)}
					onClick={() => uiStore.setDiffViewMode("scroll")}
					title={t("diffScroll.scrollView", "All files")}
					disabled={isOneSided()}
				>
					<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
						<path d="M2 2h12v1H2zm0 3h12v1H2zm0 3h10v1H2zm0 3h8v1H2z" />
					</svg>
				</button>
				<div style={{ "margin-left": "auto" }}>
					<button
						class={s.modeBtn}
						onClick={() => editorTabsStore.add(props.repoPath, props.filePath)}
						title={t("diffTab.editFile", "Edit file")}
					>
						<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
							<path d="M12.1 1.3a1.5 1.5 0 0 1 2.1 0l.5.5a1.5 1.5 0 0 1 0 2.1L5.8 12.8l-3.5.9.9-3.5L12.1 1.3zM11 3.4 4.1 10.3l-.5 1.9 1.9-.5L12.4 4.8 11 3.4z" />
						</svg>
					</button>
				</div>
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
			{/* Scroll mode: show all-files view instead of single-file diff */}
			<Show when={mode() === "scroll"}>
				<BranchDiffScrollView
					repoPath={props.repoPath}
					contentRef={(el) => {
						contentRef = el;
					}}
				/>
			</Show>
			<Show when={mode() !== "scroll"}>
				<div
					class={s.diffWrapper}
					ref={(el) => setScrollEl(el)}
					onMouseOver={(e) => {
						if (!canRestore(props.scope, props.untracked)) return;
						const idx = findHunkIndex(e.target as HTMLElement);
						setHoverHunkIdx(idx);
					}}
					onMouseOut={() => setHoverHunkIdx(null)}
					onClick={(e) => {
						const btn = (e.target as HTMLElement).closest(`.${s.revertBtn}`);
						if (btn) {
							const idx = parseInt(btn.getAttribute("data-hunk-idx") || "-1", 10);
							if (idx >= 0) handleRevertClick(idx);
						}
					}}
					onMouseDown={lineSelection.handlers.onMouseDown}
					onMouseMove={lineSelection.handlers.onMouseMove}
					onMouseUp={lineSelection.handlers.onMouseUp}
				>
					<Show
						when={!tooLarge() || forceRenderLarge()}
						fallback={
							<div class={s.largeDiffNotice}>
								<div>
									{t("diffTab.largeDiffTitle", "This diff is large")} ({diffLines().toLocaleString()}{" "}
									{t("diffTab.lines", "lines")})
								</div>
								<button class={s.largeDiffBtn} onClick={() => setForceRenderLarge(true)}>
									{t("diffTab.renderAnyway", "Render anyway")}
								</button>
							</div>
						}
					>
						<DiffViewer
							diff={diff()}
							mode={mode()}
							contentRef={(el: HTMLElement) => {
								contentRef = el;
								lineSelection.setContentRef(el);
							}}
							emptyMessage={
								loading()
									? t("diffTab.loading", "Loading diff...")
									: error()
										? `${t("diffTab.error", "Error:")} ${error()}`
										: t("diffTab.noChanges", "No changes")
							}
						/>
						{/* Floating revert buttons — rendered after diff, positioned via CSS */}
						<Show when={canRestore(props.scope, props.untracked) && diff().trim()}>
							<HunkRevertOverlay
								diff={diff()}
								contentRef={contentRef}
								hoverIdx={hoverHunkIdx()}
								isStaged={isStaged()}
							/>
						</Show>
					</Show>
					<Show when={selectedCount() > 0}>
						<div class={s.selectionFloater}>
							<Show when={commentVisible()}>
								<CommentBox
									value={commentText()}
									error={commentError()}
									onInput={setCommentText}
									onSend={handleCommentSend}
									onCancel={() => setCommentVisible(false)}
									setTextareaRef={(el) => {
										commentTextareaRef = el;
									}}
								/>
							</Show>
							<div class={s.selectionBar}>
								<Show when={canRestore(props.scope, props.untracked)}>
									<button class={s.restoreSelectedBtn} onClick={handleRestoreSelected}>
										{isStaged() ? "Unstage" : "Discard"} {selectedCount()} line{selectedCount() > 1 ? "s" : ""}
									</button>
								</Show>
								<button class={s.commentBtn} onClick={handleCommentOpen} title="Comment on selected lines">
									<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
										<path d="M1 3a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2H5.236L2.22 14.54A.5.5 0 0 1 1 14.14V3zm2-1a1 1 0 0 0-1 1v9.86l2.236-1.86A.5.5 0 0 1 4.56 11H13a1 1 0 0 0 1-1V3a1 1 0 0 0-1-1H3z" />
									</svg>
									Comment
								</button>
								<button
									class={s.clearSelectionBtn}
									onClick={() => {
										lineSelection.clear();
										setCommentVisible(false);
									}}
									title="Clear selection"
								>
									&times;
								</button>
							</div>
						</div>
					</Show>
					<Show when={searchVisible()}>
						<DomSearchOverview scrollEl={scrollEl} fractions={overviewFractions} />
					</Show>
				</div>
				<ConfirmDialog
					visible={confirmVisible()}
					title={confirmTitle()}
					message={confirmMessage()}
					confirmLabel={isStaged() ? t("diffTab.unstage", "Unstage") : t("diffTab.discard", "Discard")}
					kind={isStaged() ? "info" : "warning"}
					defaultButton="cancel"
					onConfirm={confirmRevert}
					onClose={cancelRevert}
				/>
			</Show>
		</div>
	);
};

/** Overlay that positions revert buttons on hunk header rows */
const HunkRevertOverlay: Component<{
	diff: string;
	contentRef: HTMLElement | undefined;
	hoverIdx: number | null;
	isStaged: boolean;
}> = (props) => {
	// Button positions depend on the rendered rows, not on hover. Recompute only
	// when the diff changes or the content reflows (ResizeObserver) — NOT on every
	// hoverIdx change, which previously re-ran getBoundingClientRect for every hunk.
	const [reflowTick, setReflowTick] = createSignal(0);
	createEffect(() => {
		const el = props.contentRef;
		if (!el) return;
		const ro = new ResizeObserver(() => setReflowTick((n) => n + 1));
		ro.observe(el);
		onCleanup(() => ro.disconnect());
	});

	const buttons = createMemo<Array<{ top: number; idx: number }>>(() => {
		props.diff; // track: rows rebuilt on diff change
		reflowTick(); // track: content finished rendering / reflowed
		if (!props.contentRef) return [];
		const hunkEls = props.contentRef.querySelectorAll("[class*='diff-line-hunk-content']");
		const result: Array<{ top: number; idx: number }> = [];
		const containerRect = props.contentRef.getBoundingClientRect();
		hunkEls.forEach((el, i) => {
			const row = el.closest("tr");
			if (!row) return;
			const rect = row.getBoundingClientRect();
			result.push({ top: rect.top - containerRect.top + props.contentRef!.scrollTop, idx: i });
		});
		return result;
	});

	return (
		<For each={buttons()}>
			{(b) => (
				<button
					class={cx(s.revertBtn, props.hoverIdx === b.idx && s.revertBtnVisible)}
					style={{ top: `${b.top}px` }}
					data-hunk-idx={b.idx}
					title={
						props.isStaged
							? t("diffTab.unstageHunk", "Unstage this change")
							: t("diffTab.discardHunk", "Discard this change")
					}
					aria-label={props.isStaged ? "Unstage hunk" : "Discard hunk"}
				>
					<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
						<path d="M2 8a6 6 0 1 1 12 0A6 6 0 0 1 2 8zm6-4a4 4 0 1 0 0 8 4 4 0 0 0 0-8zM6.5 7.5l3-2v4l-3-2z" />
					</svg>
				</button>
			)}
		</For>
	);
};

export default DiffTab;
