import { type Component, createEffect, createMemo, createSignal, Show } from "solid-js";
import { useRepository } from "../../hooks/useRepository";
import { t } from "../../i18n";
import { repositoriesStore } from "../../stores/repositories";
import type { DiffViewMode } from "../../stores/ui";
import { openFileAction } from "../../utils/filePreview";
import s from "../PrDiffTab/PrDiffTab.module.css";
import { DiffFileList, fileRowKeys } from "../shared/DiffFileList";
import type { DiffListNavHandle } from "../shared/diffListNav";
import { parseDiffFiles } from "../ui/DiffViewer";

export interface BranchDiffScrollViewProps {
	repoPath: string;
	/** Split vs. unified — owned by the caller (`DiffTab`), which already
	 *  resolves the shared `uiStore.diffViewMode`. `DiffViewMode` no longer has
	 *  a "scroll" variant (that's this component's own reason to exist, not a
	 *  mode a value of this type can carry), so no mapping is needed here. */
	mode: DiffViewMode;
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
 * Reactively reloads on git operations via repositoriesStore.getRevision.
 */
export const BranchDiffScrollView: Component<BranchDiffScrollViewProps> = (props) => {
	const repo = useRepository();
	const [diff, setDiff] = createSignal("");
	const [loading, setLoading] = createSignal(true);
	const [error, setError] = createSignal<string | null>(null);

	// Reactively reload when git state changes
	let diffGen = 0;
	createEffect(() => {
		const repoPath = props.repoPath;
		if (!repoPath) return;
		// Track revision for reactivity
		void repositoriesStore.getRevision(repoPath);

		if (!diff()) setLoading(true);
		setError(null);
		// A revision-bump burst can fire several loads; only the newest may settle
		// state, so a slow earlier fetch can't overwrite a fresher diff.
		const gen = ++diffGen;
		// Fetch both unstaged and staged diffs, concatenate for a full picture
		Promise.all([repo.getDiff(repoPath), repo.getDiff(repoPath, "staged")])
			.then(([unstaged, staged]) => {
				if (gen !== diffGen) return;
				// Concatenate: staged first, then unstaged (avoids duplicate files
				// since git diff and git diff --cached don't overlap)
				setDiff([staged, unstaged].filter(Boolean).join("\n"));
				setLoading(false);
			})
			.catch((err) => {
				if (gen !== diffGen) return;
				setError(String(err));
				setLoading(false);
			});
	});

	const files = createMemo(() => parseDiffFiles(diff()).filter((f) => f.additions > 0 || f.deletions > 0));
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
			<DiffFileList
				files={files()}
				mode={props.mode}
				onOpenFile={(path) => openFileAction(path, props.repoPath)}
				scrollRef={props.contentRef}
				header={summaryHeader()}
				headerHeight={35}
				collapsedKeys={collapsedKeys()}
				onToggleCollapsed={toggleCollapsed}
				ref={props.ref}
			/>
		</Show>
	);
};
