import { type Component, createEffect, createMemo, createSignal, For, on, Show, untrack } from "solid-js";
import { useRepository } from "../../hooks/useRepository";
import { t } from "../../i18n";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import { repositoriesStore } from "../../stores/repositories";
import { type DiffViewMode, uiStore } from "../../stores/ui";
import { cx } from "../../utils";
import { classifyFingerprintedDiff } from "../../utils/classifyReviewDiff";
import { diffOptionsFromSettings } from "../../utils/diffOptionsFromSettings";
import { openFileAction } from "../../utils/filePreview";
import type { CommitLogEntry } from "../GitPanel/types";
import s from "../PrDiffTab/PrDiffTab.module.css";
import { DiffFileList, fileRowKeys, sectionToRawDiff } from "../shared/DiffFileList";
import type { DiffListNavHandle } from "../shared/diffListNav";
import { Button } from "../ui/Button";
import { type DiffFileSection, parseDiffFiles } from "../ui/DiffViewer";
import { SegmentedControl } from "../ui/SegmentedControl";
import { Select } from "../ui/Select";
import { Switch } from "../ui/Switch";
import v from "./AllChangesView.module.css";

export interface AllChangesViewProps {
	repoPath: string;
	/** Split vs. unified — owned by the caller (`DiffTab`), which already
	 *  resolves the shared `uiStore.diffViewMode`. `DiffViewMode` no longer has
	 *  a "scroll" variant (that's this component's own reason to exist, not a
	 *  mode a value of this type can carry), so no mapping is needed here. */
	mode?: DiffViewMode;
	/** Pass a ref callback to get the scroll container for Cmd+F search */
	contentRef?: (el: HTMLElement) => void;
	/** Forwarded to the underlying `DiffFileList` — the `<`/`>` file-to-file toolbar
	 *  (owned by `DiffTab`, which hosts this component) drives it. */
	ref?: (handle: DiffListNavHandle) => void;
}

export type AllChangesScope = "uncommitted" | "branch" | "commits";

const COMMIT_PAGE = 50;

/**
 * A base chosen in the selector lives for the app session only: it is never
 * written to `tuicommander-base`, which also drives ahead/behind and
 * "update from base". Keyed by repository and branch.
 */
const sessionBases = new Map<string, string>();

/**
 * Selected commits by hash: the log reloads on every revision bump, and a new
 * commit shifts every index, so an index would silently point at another commit.
 */
interface CommitSelection {
	anchor: string;
	end: string;
}

/**
 * All changes of a repository in one continuous scroll, in one of three scopes:
 * uncommitted work, the branch against its base, or a commit / commit range.
 */
export const AllChangesView: Component<AllChangesViewProps> = (props) => {
	const repo = useRepository();
	const [branch, setBranch] = createSignal<string | null>(null);
	const [baseOptions, setBaseOptions] = createSignal<string[]>([]);
	const [base, setBase] = createSignal<string | null>(null);
	const [scope, setScope] = createSignal<AllChangesScope | null>(null);
	const [includeUncommitted, setIncludeUncommitted] = createSignal(false);
	const [commits, setCommits] = createSignal<CommitLogEntry[]>([]);
	const [selection, setSelection] = createSignal<CommitSelection | null>(null);
	const [diff, setDiff] = createSignal("");
	const [loading, setLoading] = createSignal(true);
	const [error, setError] = createSignal<string | null>(null);

	// This component's own nav handle — needed internally (to know which files are on
	// screen, for the live-update classification below) as well as forwarded to `props.ref`.
	const [ownHandle, setOwnHandle] = createSignal<DiffListNavHandle | null>(null);

	// Live-update bookkeeping. There is no per-file revision fingerprint (this is a plain
	// git diff, not a tracked review), so a file's reconstructed diff text stands in as
	// its fingerprint: it changes iff the file's actual diff content does.
	const [displayedFiles, setDisplayedFiles] = createSignal<DiffFileSection[]>([]);
	const [flashKeys, setFlashKeys] = createSignal<Set<string>>(new Set());
	const flashTimers = new Map<string, ReturnType<typeof setTimeout>>();
	const [pendingHiddenCount, setPendingHiddenCount] = createSignal(0);
	const [latestFetchedFiles, setLatestFetchedFiles] = createSignal<DiffFileSection[] | null>(null);
	const [hasNewFilesBelow, setHasNewFilesBelow] = createSignal(false);

	const sessionKey = () => `${props.repoPath}\0${branch() ?? ""}`;

	// Resolve branch, base and the default scope once per repository.
	// DEFERRED (2026-11-07) — a checkout while the view stays open keeps the old branch
	// and its session base key until the view remounts; not observed, not verified.
	createEffect(
		on(
			() => props.repoPath,
			async (repoPath) => {
				if (!repoPath) return;
				setScope(null);
				setSelection(null);
				try {
					const [info, options] = await Promise.all([repo.getInfo(repoPath), repo.listBaseRefOptions(repoPath)]);
					if (repoPath !== props.repoPath) return;
					const current = info.branch || null;
					const stored = current
						? await invoke<string | null>("get_branch_base", { path: repoPath, branchName: current }).catch(() => null)
						: null;
					if (repoPath !== props.repoPath) return;
					const fallback = options.find((o) => o.is_default)?.name ?? options[0]?.name ?? null;
					setBranch(current);
					const chosen = sessionBases.get(`${repoPath}\0${current ?? ""}`) ?? stored ?? fallback;
					const names = options.map((o) => o.name);
					setBaseOptions(chosen && !names.includes(chosen) ? [chosen, ...names] : names);
					setBase(chosen);
					setScope(chosen && current && current !== chosen ? "branch" : "uncommitted");
				} catch (err) {
					appLogger.warn("git", "All Changes: cannot resolve the branch base", { error: String(err) });
					setBase(null);
					setScope("uncommitted");
				}
			},
		),
	);

	/** The diff scope string of the backend grammar, or null when nothing is selected. */
	const diffScope = createMemo((): string | null => {
		switch (scope()) {
			case "uncommitted":
				return "head";
			case "branch": {
				const b = base();
				if (!b) return null;
				return includeUncommitted() ? `base-uncommitted:${b}` : `base:${b}`;
			}
			case "commits": {
				const sel = selection();
				const list = commits();
				if (!sel) return null;
				const a = list.findIndex((c) => c.hash === sel.anchor);
				const b = list.findIndex((c) => c.hash === sel.end);
				if (a < 0 || b < 0) return null;
				const newest = list[Math.min(a, b)].hash;
				const oldest = list[Math.max(a, b)].hash;
				return newest === oldest ? newest : `range:${oldest}..${newest}`;
			}
			default:
				return null;
		}
	});

	// Commit list for the Commit / range scope.
	createEffect(() => {
		const repoPath = props.repoPath;
		if (!repoPath || scope() !== "commits") return;
		void repositoriesStore.getRevision(repoPath);
		invoke<CommitLogEntry[]>("get_commit_log", { path: repoPath, count: COMMIT_PAGE })
			.then((list) => {
				if (repoPath === props.repoPath) setCommits(list);
			})
			.catch((err) => appLogger.warn("git", "All Changes: cannot load the commit log", { error: String(err) }));
	});

	// Reactively reload when git state or the scope changes.
	let diffGen = 0;
	let shownScope: string | null = null;
	createEffect(() => {
		const repoPath = props.repoPath;
		const currentScope = diffScope();
		if (!repoPath) return;
		void repositoriesStore.getRevision(repoPath);
		// Read in the effect body so a whitespace/case option change reloads the diff.
		const options = diffOptionsFromSettings();
		// A revision-bump burst can fire several loads; only the newest may settle
		// state, so a slow earlier fetch can't overwrite a fresher diff.
		const gen = ++diffGen;
		setError(null);
		if (currentScope === null) {
			shownScope = null;
			setDiff("");
			setLoading(scope() === null);
			return;
		}
		// A new scope must not show the previous scope's diff under its label;
		// a revision bump of the same scope keeps the old diff until the new one.
		if (currentScope !== shownScope) setDiff("");
		shownScope = currentScope;
		if (!untrack(diff)) setLoading(true);
		repo
			.getDiff(repoPath, currentScope, options)
			.then((text) => {
				if (gen !== diffGen) return;
				setDiff(text);
				setLoading(false);
			})
			.catch((err) => {
				if (gen !== diffGen) return;
				setError(String(err));
				setLoading(false);
			});
	});

	// DEFERRED (2026-11-07) — binary, rename-only and mode-only files have no +/- lines and stay
	// hidden (filter kept from the old view); branch and range scopes show them more often.
	const fetchedFiles = createMemo(() => parseDiffFiles(diff()).filter((f) => f.additions > 0 || f.deletions > 0));

	function flashFiles(keys: string[]) {
		if (keys.length === 0) return;
		setFlashKeys((prev) => new Set([...prev, ...keys]));
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

	/** Classifies `fresh` against what is displayed and applies it: new files and on-screen
	 *  changes apply at once (the latter with a flash), an off-screen file's change is held
	 *  behind "Refresh (N)". */
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

	// A new scope shows its own diff outright; a revision bump within the same scope goes
	// through the live-update classification so on-screen files are not yanked around.
	let appliedScope: string | null = null;
	createEffect(() => {
		const fresh = fetchedFiles();
		const currentScope = diffScope();
		untrack(() => {
			if (currentScope !== appliedScope || fresh.length === 0) {
				appliedScope = currentScope;
				setDisplayedFiles(fresh);
				setPendingHiddenCount(0);
				setLatestFetchedFiles(null);
				setHasNewFilesBelow(false);
				return;
			}
			applyFreshFiles(fresh);
		});
	});

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

	const chooseBase = (name: string) => {
		sessionBases.set(sessionKey(), name);
		setBase(name);
	};

	const selectCommit = (hash: string, extend: boolean) => {
		const current = selection();
		setSelection(extend && current ? { anchor: current.anchor, end: hash } : { anchor: hash, end: hash });
	};

	const isSelected = (index: number) => {
		const sel = selection();
		if (!sel) return false;
		const list = commits();
		const a = list.findIndex((c) => c.hash === sel.anchor);
		const b = list.findIndex((c) => c.hash === sel.end);
		return a >= 0 && b >= 0 && index >= Math.min(a, b) && index <= Math.max(a, b);
	};

	const emptyText = () => {
		if (scope() === "branch") return t("diffScroll.noBranchChanges", "No changes against the base");
		if (scope() === "commits") return t("diffScroll.selectCommit", "Select a commit, Shift-click for a range");
		return t("diffScroll.noChanges", "No uncommitted changes");
	};

	const summaryHeader = () => (
		<div>
			<div class={s.header}>
				<span class={s.headerTitle}>{t("diffScroll.title", "All Changes")}</span>
				<span class={s.headerStats}>
					{files().length} {t("diffScroll.files", "files")} <span class={s.statAdd}>+{totalAdd()}</span>{" "}
					<span class={s.statDel}>-{totalDel()}</span>
				</span>
			</div>
			<div class={v.scopeBar}>
				<SegmentedControl<AllChangesScope>
					aria-label={t("diffScroll.scope", "Scope")}
					value={scope() ?? undefined}
					onChange={setScope}
					options={[
						{ id: "uncommitted", label: t("diffScroll.scopeUncommitted", "Uncommitted") },
						{ id: "branch", label: t("diffScroll.scopeBranch", "Branch vs base"), disabled: !base() },
						{ id: "commits", label: t("diffScroll.scopeCommits", "Commit / range") },
					]}
				/>
				<Show when={scope() === "branch"}>
					<Select
						compact
						aria-label={t("diffScroll.base", "Base")}
						value={base() ?? ""}
						onChange={(e) => chooseBase(e.currentTarget.value)}
					>
						<For each={baseOptions()}>{(name) => <option value={name}>{name}</option>}</For>
					</Select>
					<label class={v.toggle}>
						<Switch
							role="checkbox"
							checked={includeUncommitted()}
							onChange={(e) => setIncludeUncommitted(e.currentTarget.checked)}
						/>
						{t("diffScroll.includeUncommitted", "Include uncommitted")}
					</label>
				</Show>
			</div>
			<Show when={scope() === "commits"}>
				<div
					class={v.commitList}
					role="listbox"
					aria-multiselectable="true"
					aria-label={t("diffScroll.commits", "Commits")}
				>
					<For each={commits()}>
						{(commit, index) => (
							<button
								type="button"
								role="option"
								data-control-layout="row"
								aria-selected={isSelected(index())}
								class={cx(v.commitRow, isSelected(index()) && v.commitRowSelected)}
								onClick={(e) => selectCommit(commit.hash, e.shiftKey)}
							>
								<span class={v.commitHash}>{commit.hash.slice(0, 7)}</span>
								<span class={v.commitSubject}>{commit.subject}</span>
							</button>
						)}
					</For>
				</div>
			</Show>
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
						<div class={s.emptyState}>{emptyText()}</div>
					</Show>
				</div>
			}
		>
			<Show when={hasNewFilesBelow() || pendingHiddenCount() > 0}>
				<div class={s.liveUpdateBar}>
					<Show when={hasNewFilesBelow()}>
						<Button size="compact" class={s.pillBtn} onClick={jumpToNewContentBelow}>
							{t("diffScroll.newContentBelow", "New content below")}
						</Button>
					</Show>
					<Show when={pendingHiddenCount() > 0}>
						<Button size="compact" class={s.pillBtn} onClick={applyPendingHiddenUpdates}>
							{t("diffScroll.refreshCount", "Refresh ({count})", { count: String(pendingHiddenCount()) })}
						</Button>
					</Show>
				</div>
			</Show>
			<DiffFileList
				files={files()}
				mode={props.mode ?? uiStore.state.diffViewMode}
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
