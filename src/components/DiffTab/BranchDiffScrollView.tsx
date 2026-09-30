import { type Component, createEffect, createMemo, createSignal, Show, untrack } from "solid-js";
import { useRepository } from "../../hooks/useRepository";
import { t } from "../../i18n";
import { repositoriesStore } from "../../stores/repositories";
import type { DiffViewMode } from "../../stores/ui";
import { classifyFingerprintedDiff } from "../../utils/classifyReviewDiff";
import { diffOptionsFromSettings } from "../../utils/diffOptionsFromSettings";
import { openFileAction } from "../../utils/filePreview";
import s from "../PrDiffTab/PrDiffTab.module.css";
import { DiffFileList, fileRowKeys, sectionToRawDiff } from "../shared/DiffFileList";
import type { DiffListNavHandle } from "../shared/diffListNav";
import { type DiffFileSection, parseDiffFiles } from "../ui/DiffViewer";

export interface BranchDiffScrollViewProps {
	repoPath: string;
	/** Split vs. unified — owned by the caller (`DiffTab`), which already
	 *  resolves the shared `uiStore.diffViewMode`. `DiffViewMode` no longer has
	 *  a "scroll" variant (that's this component's own reason to exist, not a
	 *  mode a value of this type can carry), so no mapping is needed here. */
	mode: DiffViewMode;
	wrap?: boolean;
	maxLines?: number;
	/** Pass a ref callback to get the scroll container for Cmd+F search */
	contentRef?: (el: HTMLElement) => void;
	/** Forwarded straight to the underlying `DiffFileList` — a `<`/`>`
	 *  file-to-file toolbar (owned by `DiffTab`, which hosts this component)
	 *  drives it. */
	ref?: (handle: DiffListNavHandle) => void;
}

/**
 * All-files diff scroll view for the current working tree.
 * Shows every changed file as a collapsible section in a continuous scroll.
 * Reactively reloads on git operations via repositoriesStore.getRevision, and
 * on a whitespace/case diff-option change.
 */
export const BranchDiffScrollView: Component<BranchDiffScrollViewProps> = (props) => {
	const repo = useRepository();
	const [displayedFiles, setDisplayedFiles] = createSignal<DiffFileSection[]>([]);
	const [loading, setLoading] = createSignal(true);
	const [error, setError] = createSignal<string | null>(null);

	// This component's own nav handle — needed internally (to know what's
	// currently visible, for the live-update classification below) as well as
	// forwarded to `props.ref` for `DiffTab`'s `<`/`>` toolbar.
	const [ownHandle, setOwnHandle] = createSignal<DiffListNavHandle | null>(null);

	// Live-update bookkeeping — the same shape as SessionDiffTab's, but Branch
	// Diff Scroll has no per-file `revision` fingerprint of its own (nothing
	// backs one — it's a plain git diff, not a tracked review), so the
	// reconstructed diff text for each file stands in as its fingerprint: it
	// changes iff the file's actual diff content does.
	const [flashKeys, setFlashKeys] = createSignal<Set<string>>(new Set());
	const flashTimers = new Map<string, ReturnType<typeof setTimeout>>();
	const [pendingHiddenCount, setPendingHiddenCount] = createSignal(0);
	const [latestFetchedFiles, setLatestFetchedFiles] = createSignal<DiffFileSection[] | null>(null);
	const [hasNewFilesBelow, setHasNewFilesBelow] = createSignal(false);

	function flashFiles(keys: string[]) {
		if (keys.length === 0) return;
		setFlashKeys((prev) => {
			const next = new Set(prev);
			for (const k of keys) next.add(k);
			return next;
		});
		for (const k of keys) {
			const existing = flashTimers.get(k);
			if (existing) clearTimeout(existing);
			flashTimers.set(
				k,
				setTimeout(() => {
					flashTimers.delete(k);
					setFlashKeys((prev) => {
						if (!prev.has(k)) return prev;
						const next = new Set(prev);
						next.delete(k);
						return next;
					});
				}, 1500),
			);
		}
	}

	function nearBottom(): boolean {
		const handle = ownHandle();
		if (!handle) return true;
		return handle.currentIndex() >= handle.rowCount() - 2;
	}

	function applyPendingHiddenUpdates() {
		const fresh = latestFetchedFiles();
		if (!fresh) return;
		setDisplayedFiles(fresh);
		setPendingHiddenCount(0);
		setLatestFetchedFiles(null);
	}

	function jumpToNewContentBelow() {
		const handle = ownHandle();
		handle?.scrollToIndex(Math.max(handle.rowCount() - 1, 0), { align: "end" });
		setHasNewFilesBelow(false);
	}

	// Reactively reload when git state changes, or a whitespace/case diff
	// option changes (`diffOptionsFromSettings()` is read directly in this
	// effect body, so its 4 underlying settings are tracked automatically).
	let diffGen = 0;
	createEffect(() => {
		const repoPath = props.repoPath;
		if (!repoPath) return;
		void repositoriesStore.getRevision(repoPath);
		const options = diffOptionsFromSettings();

		// `untrack`ed: `displayedFiles` is this SAME effect's own output (unlike
		// the plain-string `diff` signal this replaced, a fresh array from
		// `parseDiffFiles` is never reference-equal to the last one even when its
		// content is identical) — reading it as a tracked dependency here would
		// make every `setDisplayedFiles` call re-trigger this effect, which
		// would re-fetch, which would call `setDisplayedFiles` again... forever.
		if (untrack(displayedFiles).length === 0) setLoading(true);
		setError(null);
		// A revision-bump burst can fire several loads; only the newest may settle
		// state, so a slow earlier fetch can't overwrite a fresher diff.
		const gen = ++diffGen;
		// Fetch both unstaged and staged diffs, concatenate for a full picture
		Promise.all([repo.getDiff(repoPath, undefined, options), repo.getDiff(repoPath, "staged", options)])
			.then(([unstaged, staged]) => {
				if (gen !== diffGen) return;
				// Concatenate: staged first, then unstaged (avoids duplicate files
				// since git diff and git diff --cached don't overlap)
				const raw = [staged, unstaged].filter(Boolean).join("\n");
				const fresh = parseDiffFiles(raw).filter((f) => f.additions > 0 || f.deletions > 0);
				applyFreshFiles(fresh);
				setLoading(false);
			})
			.catch((err) => {
				if (gen !== diffGen) return;
				setError(String(err));
				setLoading(false);
			});
	});

	/** Classifies `fresh` against what's currently displayed and applies it —
	 *  new files and on-screen changes apply immediately (the latter with a
	 *  flash), an off-screen file's change is held behind "Refresh (N)". Keys
	 *  are `fileRowKeys()`'s occurrence-order disambiguation (same scheme the
	 *  collapse-state map below already relies on being stable across
	 *  fetches — staged-before-unstaged ordering doesn't change). */
	function applyFreshFiles(fresh: DiffFileSection[]) {
		const current = displayedFiles();
		if (current.length === 0) {
			setDisplayedFiles(fresh);
			return;
		}
		const currentKeys = fileRowKeys(current);
		const freshKeys = fileRowKeys(fresh);
		const visible = ownHandle()?.visibleIndices() ?? new Set<number>();
		const visibleKeys = new Set([...visible].map((i) => currentKeys[i]).filter((k): k is string => k !== undefined));

		const { newKeys, changedVisible, changedHidden } = classifyFingerprintedDiff(
			current.map((f, i) => ({ key: currentKeys[i], fingerprint: sectionToRawDiff(f) })),
			fresh.map((f, i) => ({ key: freshKeys[i], fingerprint: sectionToRawDiff(f) })),
			visibleKeys,
		);

		if (changedHidden.length === 0) {
			setDisplayedFiles(fresh);
			setPendingHiddenCount(0);
			setLatestFetchedFiles(null);
		} else {
			const oldByKey = new Map(current.map((f, i) => [currentKeys[i], f]));
			const hiddenSet = new Set(changedHidden);
			const hybrid = fresh.map((f, i) => (hiddenSet.has(freshKeys[i]) ? (oldByKey.get(freshKeys[i]) ?? f) : f));
			setDisplayedFiles(hybrid);
			setPendingHiddenCount(changedHidden.length);
			setLatestFetchedFiles(fresh);
		}
		if (changedVisible.length > 0) flashFiles(changedVisible);
		if (newKeys.length > 0 && !nearBottom()) setHasNewFilesBelow(true);
	}

	const files = displayedFiles;
	const totalAdd = createMemo(() => files().reduce((sum, f) => sum + f.additions, 0));
	const totalDel = createMemo(() => files().reduce((sum, f) => sum + f.deletions, 0));

	// Owns collapse state here (rather than letting DiffFileList fall back to
	// its own internal signal) so a refresh that drops a file also drops its
	// now-meaningless collapsed entry, instead of growing forever.
	const [collapsedKeys, setCollapsedKeys] = createSignal<Set<string>>(new Set());
	createEffect(() => {
		const live = new Set(fileRowKeys(files()));
		setCollapsedKeys((prev) => {
			const next = new Set([...prev].filter((k) => live.has(k)));
			return next.size === prev.size ? prev : next;
		});
	});
	const toggleCollapsed = (key: string) => {
		setCollapsedKeys((prev) => {
			const next = new Set(prev);
			if (next.has(key)) next.delete(key);
			else next.add(key);
			return next;
		});
	};

	const summaryHeader = () => (
		<div class={s.header}>
			<span class={s.headerTitle}>{t("diffScroll.title", "All Changes")}</span>
			<span class={s.headerStats}>
				{files().length} {t("diffScroll.files", "files")} <span class={s.statAdd}>+{totalAdd()}</span>{" "}
				<span class={s.statDel}>-{totalDel()}</span>
			</span>
		</div>
	);

	return (
		<Show
			when={!loading() && !error() && files().length > 0}
			fallback={
				<div class={s.container} ref={props.contentRef}>
					{summaryHeader()}
					<Show when={loading()}>
						<div class={s.emptyState}>{t("diffTab.loading", "Loading diff...")}</div>
					</Show>
					<Show when={error()}>
						<div class={s.emptyState}>
							{t("diffTab.error", "Error:")} {error()}
						</div>
					</Show>
					<Show when={!loading() && !error() && files().length === 0}>
						<div class={s.emptyState}>{t("diffScroll.noChanges", "No uncommitted changes")}</div>
					</Show>
				</div>
			}
		>
			<Show when={hasNewFilesBelow() || pendingHiddenCount() > 0}>
				<div class={s.liveUpdateBar}>
					<Show when={hasNewFilesBelow()}>
						<button type="button" class={s.pillBtn} onClick={jumpToNewContentBelow}>
							{t("diffScroll.newContentBelow", "New content below")}
						</button>
					</Show>
					<Show when={pendingHiddenCount() > 0}>
						<button type="button" class={s.pillBtn} onClick={applyPendingHiddenUpdates}>
							{t("diffScroll.refreshCount", "Refresh ({count})", { count: String(pendingHiddenCount()) })}
						</button>
					</Show>
				</div>
			</Show>
			<DiffFileList
				files={files()}
				mode={props.mode}
				wrap={props.wrap}
				maxLines={props.maxLines}
				flashKeys={flashKeys()}
				onOpenFile={(path) => openFileAction(path, props.repoPath)}
				scrollRef={props.contentRef}
				header={summaryHeader()}
				headerHeight={35}
				collapsedKeys={collapsedKeys()}
				onToggleCollapsed={toggleCollapsed}
				ref={(handle) => {
					setOwnHandle(handle);
					props.ref?.(handle);
				}}
			/>
		</Show>
	);
};
